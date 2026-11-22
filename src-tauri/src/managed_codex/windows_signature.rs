use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::Cryptography::{
    CertNameToStrW, CERT_CONTEXT, CERT_NAME_STR_REVERSE_FLAG, CERT_X500_NAME_STR,
};
use windows_sys::Win32::Security::WinTrust::{
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

pub(super) struct AuthenticodeSigner {
    pub(super) subject: String,
    pub(super) certificate_sha256: String,
}

pub(super) fn verify(executable: &Path) -> Result<AuthenticodeSigner, String> {
    let mut path: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return Err("Authenticode path contains a null character".to_string());
    }
    path.push(0);

    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path.as_ptr(),
        hFile: null_mut(),
        pgKnownSubject: null_mut(),
    };
    let mut policy = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let mut trust = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: null_mut(),
        pSIPClientData: null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: null_mut(),
        pwszURLReference: null_mut(),
        dwProvFlags: 0,
        dwUIContext: 0,
        pSignatureSettings: null_mut(),
    };

    // SAFETY: All input structures and the NUL-terminated path remain live through CLOSE.
    // Copy the verified leaf's identity before releasing provider state on either outcome.
    unsafe {
        let status = WinVerifyTrust(
            null_mut(),
            &mut policy,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        );
        let result = if status == 0 {
            verified_signer(trust.hWVTStateData)
        } else {
            let message = std::io::Error::from_raw_os_error(status);
            Err(format!(
                "Authenticode verification failed (0x{:08X}): {message}",
                status as u32
            ))
        };
        trust.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(
            null_mut(),
            &mut policy,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        );
        result
    }
}

/// `state` must come from a successful WinVerifyTrust call and remain open.
unsafe fn verified_signer(state: HANDLE) -> Result<AuthenticodeSigner, String> {
    // SAFETY: WinTrust owns these records until CLOSE. Index zero selects the file
    // signer and its leaf certificate. Check each returned pointer before using it.
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return Err("Authenticode provider data missing".to_string());
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() {
            return Err("Authenticode signer missing".to_string());
        }
        let cert = WTHelperGetProvCertFromChain(signer, 0)
            .as_ref()
            .and_then(|entry| entry.pCert.as_ref())
            .ok_or_else(|| "Authenticode signer certificate missing".to_string())?;
        if cert.pbCertEncoded.is_null() || cert.cbCertEncoded == 0 {
            return Err("Authenticode signer certificate is empty".to_string());
        }
        let encoded = std::slice::from_raw_parts(cert.pbCertEncoded, cert.cbCertEncoded as usize);
        Ok(AuthenticodeSigner {
            subject: certificate_subject(cert)?,
            certificate_sha256: format!("{:x}", Sha256::digest(encoded)),
        })
    }
}

/// `cert` must be a live WinTrust certificate context.
unsafe fn certificate_subject(cert: &CERT_CONTEXT) -> Result<String, String> {
    let name_format = CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG;
    // SAFETY: The certificate context and info are live WinTrust data. The first
    // call sizes the UTF-16 buffer, including its terminating NUL.
    unsafe {
        let info = cert
            .pCertInfo
            .as_ref()
            .ok_or_else(|| "Authenticode signer certificate info missing".to_string())?;
        let size = CertNameToStrW(
            cert.dwCertEncodingType,
            &info.Subject,
            name_format,
            null_mut(),
            0,
        );
        if size == 0 {
            return Err("Authenticode signer certificate subject could not be read".to_string());
        }
        let mut subject = vec![0; size as usize];
        let written = CertNameToStrW(
            cert.dwCertEncodingType,
            &info.Subject,
            name_format,
            subject.as_mut_ptr(),
            size,
        );
        if written != size {
            return Err("Authenticode signer certificate subject could not be read".to_string());
        }
        Ok(String::from_utf16_lossy(&subject[..subject.len() - 1]))
    }
}

#[cfg(test)]
mod tests {
    use super::verify;

    #[test]
    fn rejects_unsigned_file_with_a_native_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("无签名 runtime.exe");
        std::fs::write(&path, b"unsigned").unwrap();

        assert!(
            matches!(verify(&path), Err(error) if error.starts_with("Authenticode verification failed (0x"))
        );
    }
}
