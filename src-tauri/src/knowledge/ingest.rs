pub(crate) mod conversion;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;

use super::store::ingest::{ImportReceipt, IngestJob};
use super::store::{KnowledgeStore, StoreResult};
use super::types::{
    is_knowledge_base_id, is_knowledge_job_id, is_knowledge_source_id, KnowledgeErrorCode,
};
use crate::document_processing::ManagedDocumentProcessing;
use conversion::ConversionError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub code: KnowledgeErrorCode,
    pub suggestion: &'static str,
}

impl From<KnowledgeErrorCode> for ImportFailure {
    fn from(code: KnowledgeErrorCode) -> Self {
        use KnowledgeErrorCode::*;
        let suggestion = match code {
            KbSourceUnsupported => "chooseSupportedFile",
            KbLimitExceeded => "checkImportLimits",
            KbInsufficientSpace => "freeDiskSpace",
            KbJobConflict => "refreshJob",
            KbInvalidInput | KbAccessDenied => "chooseReadableRegularFile",
            KbNotFound => "refreshLibrary",
            _ => "checkStorageAndRetry",
        };
        Self { code, suggestion }
    }
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum ImportOutcome {
    Accepted { receipt: ImportReceipt },
    Rejected { error: ImportFailure },
}

#[derive(Default)]
struct Queue {
    recovered: bool,
    closed: bool,
    pending: VecDeque<String>,
    active: Option<(String, Arc<AtomicBool>)>,
}

pub struct KnowledgeIngestManager {
    store: Arc<KnowledgeStore>,
    documents: Option<ManagedDocumentProcessing>,
    queue: Mutex<Queue>,
    settled: Condvar,
}

impl KnowledgeIngestManager {
    pub fn new(store: Arc<KnowledgeStore>, documents: Option<ManagedDocumentProcessing>) -> Self {
        Self {
            store,
            documents,
            queue: Mutex::default(),
            settled: Condvar::new(),
        }
    }

    fn queue(&self) -> StoreResult<MutexGuard<'_, Queue>> {
        self.queue
            .lock()
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)
    }

    fn open_queue(&self) -> StoreResult<MutexGuard<'_, Queue>> {
        let mut queue = self.queue()?;
        if queue.closed {
            return Err(KnowledgeErrorCode::KbStoreUnavailable);
        }
        if !queue.recovered {
            self.interrupt_unfinished()?;
            queue.recovered = true;
        }
        Ok(queue)
    }

    pub fn recover(&self) -> StoreResult<()> {
        drop(self.open_queue()?);
        Ok(())
    }

    pub fn import_files(self: &Arc<Self>, kb_id: &str, paths: Vec<PathBuf>) -> Vec<ImportOutcome> {
        paths
            .into_iter()
            .map(|path| match self.admit(kb_id, &path) {
                Ok(receipt) => ImportOutcome::Accepted { receipt },
                Err(code) => ImportOutcome::Rejected { error: code.into() },
            })
            .collect()
    }

    fn admit(self: &Arc<Self>, kb_id: &str, path: &Path) -> StoreResult<ImportReceipt> {
        if !is_knowledge_base_id(kb_id) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        let mut queue = self.open_queue()?;
        let receipt = self.store.enqueue_file(kb_id, path)?;
        if let Some(job) = &receipt.job {
            if !receipt.duplicate {
                queue.pending.push_back(job.id.clone());
                self.start_next(&mut queue);
            }
        }
        Ok(receipt)
    }

    pub fn retry(self: &Arc<Self>, kb_id: &str, source_id: &str) -> StoreResult<IngestJob> {
        if !is_knowledge_base_id(kb_id) || !is_knowledge_source_id(source_id) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        let mut queue = self.open_queue()?;
        let job = self.store.retry_ingest(kb_id, source_id)?;
        queue.pending.push_back(job.id.clone());
        self.start_next(&mut queue);
        Ok(job)
    }

    fn start_next(self: &Arc<Self>, queue: &mut Queue) {
        if queue.closed || queue.active.is_some() {
            return;
        }
        let Some(id) = queue.pending.pop_front() else {
            return;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        queue.active = Some((id.clone(), Arc::clone(&cancelled)));
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            manager.run_one(&id, &cancelled);
            if let Ok(mut queue) = manager.queue() {
                queue.active = None;
                manager.start_next(&mut queue);
                manager.settled.notify_all();
            }
        });
    }

    fn run_one(&self, id: &str, cancelled: &AtomicBool) {
        // Each settlement checks the attempt ID and current phase under the
        // Store lock. A late worker cannot override a cancellation or retry.
        match self.convert_one(id, cancelled) {
            Ok(()) => {}
            Err(ConversionError::Failed(code, suggestion)) => {
                let _ = self.store.fail_ingest(id, code, suggestion);
            }
            Err(ConversionError::Cancelled) => {
                if self.queue().is_ok_and(|queue| queue.closed) {
                    let _ = self.store.interrupt_ingest(id);
                } else {
                    let _ = self.store.cancel_ingest(id);
                }
            }
            Err(ConversionError::Interrupted) => {
                let _ = self.store.interrupt_ingest(id);
            }
        }
    }

    fn convert_one(&self, id: &str, cancelled: &AtomicBool) -> Result<(), ConversionError> {
        let failure = |code| ConversionError::Failed(code, "checkStorageAndRetry");
        let input = self.store.begin_ingest(id).map_err(failure)?;
        conversion::convert(
            &input.original,
            input.stage.path(),
            self.documents.as_ref(),
            cancelled,
            |document_id| self.store.attach_conversion(id, document_id),
        )?;
        self.store
            .publish_ingest(id, input.stage.path())
            .map_err(failure)?;
        Ok(())
    }

    pub fn status(&self, id: &str) -> StoreResult<IngestJob> {
        if !is_knowledge_job_id(id) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        self.recover()?;
        self.store.ingest_job(id)
    }

    pub fn cancel(&self, id: &str) -> StoreResult<IngestJob> {
        if !is_knowledge_job_id(id) {
            return Err(KnowledgeErrorCode::KbInvalidInput);
        }
        let mut queue = self.open_queue()?;
        let job = self.store.cancel_ingest(id)?;
        queue.pending.retain(|queued| queued != id);
        if let Some((active, cancelled)) = &queue.active {
            if active == id {
                cancelled.store(true, Ordering::Release);
            }
        }
        Ok(job)
    }

    fn interrupt_unfinished(&self) -> StoreResult<()> {
        let documents = self.store.interrupt_ingests()?;
        if let Some(manager) = &self.documents {
            for document in documents {
                let _ = manager.cancel(&document);
            }
        }
        Ok(())
    }

    pub fn shutdown(&self) -> StoreResult<()> {
        let mut queue = self.queue()?;
        queue.closed = true;
        queue.pending.clear();
        if let Some((_, cancelled)) = &queue.active {
            cancelled.store(true, Ordering::Release);
        }
        // The converter may still own its staging directory and AnyDoc job.
        // Wait for it to relinquish both before reclaiming interrupted work.
        let (queue, _) = self
            .settled
            .wait_timeout_while(queue, Duration::from_secs(30), |state| {
                state.active.is_some()
            })
            .map_err(|_| KnowledgeErrorCode::KbStoreUnavailable)?;
        if queue.active.is_some() {
            return Err(KnowledgeErrorCode::KbStoreUnavailable);
        }
        self.interrupt_unfinished()
    }
}
