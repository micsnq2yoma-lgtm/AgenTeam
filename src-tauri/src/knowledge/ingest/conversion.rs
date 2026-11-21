//! Convert one Store-owned original in a private, single-attempt directory.
//! Only the Store publishes the resulting content and assets to a source.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::document_processing::{
    reject_link_ancestors, supported_extension, DocumentJob, DocumentJobState,
    DocumentSubmitRequest, ManagedDocumentProcessing, MAX_OUTPUT_BYTES, MAX_SOURCE_BYTES,
};
use crate::durable_fs::rename_directory_noreplace;
use crate::knowledge::types::KnowledgeErrorCode;
use crate::workspace_files::path_safety::{
    create_workspace_file_no_follow, read_workspace_file_no_follow,
};

use KnowledgeErrorCode::{
    KbAccessDenied, KbInsufficientSpace, KbInvalidInput, KbLimitExceeded, KbSourceUnsupported,
    KbStoreUnavailable,
};

const TEXT_LIMIT: u64 = 8 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConversionError {
    Cancelled,
    Interrupted,
    Failed(KnowledgeErrorCode, &'static str),
}

use ConversionError::Failed;

const UNSUPPORTED: ConversionError = Failed(KbSourceUnsupported, "chooseSupportedFile");
const INVALID_TEXT: ConversionError = Failed(KbInvalidInput, "saveAsUnicodeText");
const INVALID_PATH: ConversionError = Failed(KbInvalidInput, "chooseReadableRegularFile");
const STORAGE_ERROR: ConversionError = Failed(KbStoreUnavailable, "checkStorageAndRetry");
const MISSING_RESOURCES: ConversionError = Failed(KbStoreUnavailable, "installDocumentResources");

pub(crate) fn source_limit(path: &Path) -> Result<u64, ConversionError> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| {
            ["md", "markdown", "txt"]
                .iter()
                .any(|text| ext.eq_ignore_ascii_case(text))
        })
    {
        Ok(TEXT_LIMIT)
    } else if supported_extension(path).is_some() {
        Ok(MAX_SOURCE_BYTES)
    } else {
        Err(UNSUPPORTED)
    }
}

/// Runs on the ingestion manager's blocking worker. The caller owns `stage`
/// until the Store commits it and must discard that whole attempt on any error.
pub(super) fn convert(
    original: &Path,
    stage: &Path,
    documents: Option<&ManagedDocumentProcessing>,
    cancelled: &AtomicBool,
    worker_started: impl FnOnce(&str) -> Result<(), KnowledgeErrorCode>,
) -> Result<(), ConversionError> {
    check_cancelled(cancelled)?;
    let limit = source_limit(original)?;
    if !original.is_absolute() || !stage.is_absolute() {
        return Err(INVALID_PATH);
    }
    reject_link_ancestors(original, false).map_err(|_| INVALID_PATH)?;
    reject_link_ancestors(stage, false).map_err(|_| STORAGE_ERROR)?;
    if limit == TEXT_LIMIT {
        let parent = original.parent().ok_or(INVALID_PATH)?;
        let name = original
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(INVALID_PATH)?;
        if fs::symlink_metadata(original)
            .map_err(|_| INVALID_PATH)?
            .len()
            > limit
        {
            return Err(Failed(KbLimitExceeded, "checkImportLimits"));
        }
        let (_, bytes) =
            read_workspace_file_no_follow(parent, name, limit).map_err(|_| INVALID_PATH)?;
        let text = decode_text(bytes)?;
        check_cancelled(cancelled)?;
        create_workspace_file_no_follow(stage, "content.md", text.as_bytes())
            .map_err(|_| STORAGE_ERROR)?;
        return check_cancelled(cancelled);
    }

    let manager = documents.ok_or(MISSING_RESOURCES)?;
    let mut job = manager
        .submit(DocumentSubmitRequest {
            source_path: original.to_str().ok_or(INVALID_PATH)?.to_owned(),
            output_root: Some(stage.to_str().ok_or(STORAGE_ERROR)?.to_owned()),
            current_workspace: None,
            password: None,
        })
        .map_err(|error| document_error(&error.code))?;
    // A metadata write can lose a race with cancel/shutdown. Keep owning the
    // attempt until AnyDoc acknowledges the cancellation or reaches a terminal.
    let recording_error = worker_started(&job.job_id).err();
    while !job.state.is_terminal() {
        if (recording_error.is_some() || cancelled.load(Ordering::Acquire))
            && job.state != DocumentJobState::Cancelling
        {
            // Another owner can finish/cancel between snapshots. get() below
            // resolves that race against the manager's authoritative state.
            let _ = manager.cancel(&job.job_id);
        }
        std::thread::sleep(POLL_INTERVAL);
        job = manager
            .get(&job.job_id)
            .map_err(|error| document_error(&error.code))?;
    }
    if let Some(code) = recording_error {
        return Err(Failed(code, "checkStorageAndRetry"));
    }
    check_cancelled(cancelled)?;
    match job.state {
        DocumentJobState::Succeeded | DocumentJobState::SucceededWithWarnings => {
            adopt_artifacts(stage, &job)?;
            check_cancelled(cancelled)
        }
        DocumentJobState::Cancelled => Err(ConversionError::Cancelled),
        DocumentJobState::Interrupted => Err(ConversionError::Interrupted),
        _ => Err(document_error(
            job.error.as_ref().map_or("", |error| error.code.as_str()),
        )),
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), ConversionError> {
    if cancelled.load(Ordering::Acquire) {
        Err(ConversionError::Cancelled)
    } else {
        Ok(())
    }
}

fn decode_text(bytes: Vec<u8>) -> Result<String, ConversionError> {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return Err(INVALID_TEXT);
    }
    let utf16 = if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        Some((body, u16::from_le_bytes as fn([u8; 2]) -> u16))
    } else {
        bytes
            .strip_prefix(&[0xfe, 0xff])
            .map(|body| (body, u16::from_be_bytes as fn([u8; 2]) -> u16))
    };
    let text = if let Some((body, decode)) = utf16 {
        let units = body.chunks_exact(2);
        if !units.remainder().is_empty() {
            return Err(INVALID_TEXT);
        }
        char::decode_utf16(units.map(|pair| decode([pair[0], pair[1]])))
            .collect::<Result<String, _>>()
            .map_err(|_| INVALID_TEXT)?
    } else {
        let mut text = String::from_utf8(bytes).map_err(|_| INVALID_TEXT)?;
        if text.starts_with('\u{feff}') {
            text.drain(..'\u{feff}'.len_utf8());
        }
        text
    };
    if text.contains('\0') {
        return Err(INVALID_TEXT);
    }
    Ok(text)
}

fn document_error(code: &str) -> ConversionError {
    match code {
        "DOCUMENT_CANCELLED" => ConversionError::Cancelled,
        "DOCUMENT_INTERRUPTED" | "DOCUMENT_MANAGER_SHUTTING_DOWN" => ConversionError::Interrupted,
        "DOCUMENT_UNSUPPORTED_FORMAT" => UNSUPPORTED,
        "DOCUMENT_RESOURCE_LIMIT" | "DOCUMENT_TIMEOUT" => Failed(KbLimitExceeded, "splitDocument"),
        "DOCUMENT_QUEUE_FULL" => Failed(KbLimitExceeded, "waitForConversion"),
        "DOCUMENT_SOURCE_PERMISSION_DENIED" => Failed(KbAccessDenied, "chooseReadableRegularFile"),
        "DOCUMENT_PASSWORD_REQUIRED"
        | "DOCUMENT_PASSWORD_INVALID"
        | "DOCUMENT_ENCRYPTION_SCHEME_UNSUPPORTED" => {
            Failed(KbInvalidInput, "provideUnencryptedCopy")
        }
        "DOCUMENT_NO_USABLE_CONTENT" | "DOCUMENT_OCR_NO_TEXT" => {
            Failed(KbInvalidInput, "chooseClearerDocument")
        }
        "DOCUMENT_MALFORMED" | "DOCUMENT_SOURCE_CHANGED" | "DOCUMENT_INPUT_READ_FAILED" => {
            Failed(KbInvalidInput, "resaveDocument")
        }
        "DOCUMENT_OCR_RUNTIME_UNAVAILABLE"
        | "DOCUMENT_PDFIUM_LOAD_FAILED"
        | "DOCUMENT_RUNTIME_UNSUPPORTED_PLATFORM"
        | "DOCUMENT_RESOURCE_MISSING"
        | "DOCUMENT_RESOURCE_INVALID"
        | "DOCUMENT_RESOURCE_MANIFEST_INVALID"
        | "DOCUMENT_RESOURCE_TARGET_MISMATCH" => MISSING_RESOURCES,
        "DOCUMENT_INSUFFICIENT_DISK_SPACE" => Failed(KbInsufficientSpace, "freeDiskSpace"),
        "DOCUMENT_OUTPUT_UNWRITABLE"
        | "DOCUMENT_PRIVATE_STORAGE_UNAVAILABLE"
        | "DOCUMENT_JOB_STORE_WRITE_FAILED" => STORAGE_ERROR,
        _ => Failed(KbStoreUnavailable, "checkDocumentConverter"),
    }
}

fn adopt_artifacts(stage: &Path, job: &DocumentJob) -> Result<(), ConversionError> {
    if !job.output.artifact_available {
        return Err(STORAGE_ERROR);
    }
    // The ID comes from the job we submitted. Derive paths from our own root;
    // paths serialized in the document job are descriptive, never authority.
    let published = stage.join(&job.job_id);
    reject_link_ancestors(&published, false).map_err(|_| STORAGE_ERROR)?;
    // Take the complete published directory in one rename, so AnyDoc's public
    // path can never expose a document whose assets are already being adopted.
    let adopted = stage.join(".converted");
    rename_directory_noreplace(&published, &adopted).map_err(|_| STORAGE_ERROR)?;
    let (_, markdown) = read_workspace_file_no_follow(&adopted, "document.md", MAX_OUTPUT_BYTES)
        .map_err(|_| STORAGE_ERROR)?;
    if job.output.assets_directory.is_some() {
        let assets = adopted.join("assets");
        reject_link_ancestors(&assets, false).map_err(|_| STORAGE_ERROR)?;
        if !fs::symlink_metadata(&assets).is_ok_and(|meta| meta.is_dir()) {
            return Err(STORAGE_ERROR);
        }
        rename_directory_noreplace(&assets, &stage.join("assets")).map_err(|_| STORAGE_ERROR)?;
    }
    create_workspace_file_no_follow(stage, "content.md", &markdown).map_err(|_| STORAGE_ERROR)?;
    Ok(())
}

#[cfg(test)]
mod tests;
