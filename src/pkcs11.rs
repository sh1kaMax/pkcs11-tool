use std::path::{Path, PathBuf};

use cryptoki::{
    context::{CInitializeArgs, CInitializeFlags, Pkcs11},
    error::Error as CryptokiError,
    object::{Attribute, AttributeType, ObjectClass, ObjectHandle},
    session::{Session, UserType},
    slot::Slot,
    types::AuthPin,
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct TokenSummary {
    pub slot: Slot,
    pub label: String,
    pub serial: String,
    pub manufacturer: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct TokenObjectInfo {
    pub handle: ObjectHandle,
    pub label: String,
    pub class_name: String,
    pub size: usize,
}

pub struct Pkcs11Service {
    context: Pkcs11,
}

pub struct TokenSession {
    pub token: TokenSummary,
    session: Session,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("PKCS#11 module not found: {0}")]
    ModuleMissing(String),
    #[error("PKCS#11 error: {0}")]
    Cryptoki(#[from] CryptokiError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl Pkcs11Service {
    pub fn new(module_path: impl Into<PathBuf>) -> Result<Self, ServiceError> {
        let module_path = module_path.into();
        if !module_path.exists() {
            return Err(ServiceError::ModuleMissing(module_path.display().to_string()));
        }

        let context = Pkcs11::new(module_path.as_path())?;
        context.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))?;

        Ok(Self { context })
    }

    pub fn list_tokens(&self) -> Result<Vec<TokenSummary>, ServiceError> {
        let mut items = Vec::new();
        for slot in self.context.get_slots_with_token()? {
            let info = self.context.get_token_info(slot)?;
            items.push(TokenSummary {
                slot,
                label: info.label().trim().to_owned(),
                serial: info.serial_number().trim().to_owned(),
                manufacturer: info.manufacturer_id().trim().to_owned(),
                model: info.model().trim().to_owned(),
            });
        }
        Ok(items)
    }

    pub fn login(&self, token: TokenSummary, pin: &str) -> Result<TokenSession, ServiceError> {
        let session = self.context.open_rw_session(token.slot)?;
        session.login(
            UserType::User,
            Some(&AuthPin::new(pin.to_owned().into_boxed_str())),
        )?;
        Ok(TokenSession { token, session })
    }
}

impl TokenSession {
    pub fn format(&self) -> Result<usize, ServiceError> {
        let objects = self.collect_object_handles(&[])?;
        let mut removed = 0usize;
        for handle in objects {
            if self.session.destroy_object(handle).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn change_pin(&self, old_pin: &str, new_pin: &str) -> Result<(), ServiceError> {
        self.session.set_pin(
            &AuthPin::new(old_pin.to_owned().into_boxed_str()),
            &AuthPin::new(new_pin.to_owned().into_boxed_str()),
        )?;
        Ok(())
    }

    pub fn write_file(&self, label: &str, source_path: &Path) -> Result<(), ServiceError> {
        let data = std::fs::read(source_path)?;
        let file_name = source_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("payload.bin")
            .as_bytes()
            .to_vec();

        let attrs = vec![
            Attribute::Class(ObjectClass::DATA),
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Modifiable(true),
            Attribute::Label(label.as_bytes().to_vec()),
            Attribute::Application(b"PKCS11 Token Studio".to_vec()),
            Attribute::ObjectId(file_name),
            Attribute::Value(data),
        ];

        self.session.create_object(&attrs)?;
        Ok(())
    }

    pub fn list_objects(&self) -> Result<Vec<TokenObjectInfo>, ServiceError> {
        let handles = self.collect_object_handles(&[Attribute::Class(ObjectClass::DATA)])?;

        let mut objects = Vec::new();
        for handle in handles {
            let attrs = self
                .session
                .get_attributes(handle, &[AttributeType::Label, AttributeType::Class, AttributeType::Value])?;
            let mut label = String::from("Unnamed");
            let mut class_name = String::from("DATA");
            let mut size = 0usize;

            for attr in attrs {
                match attr {
                    Attribute::Label(bytes) => {
                        label = String::from_utf8_lossy(&bytes).trim().to_owned();
                    }
                    Attribute::Class(class) => {
                        class_name = format!("{class:?}");
                    }
                    Attribute::Value(bytes) => {
                        size = bytes.len();
                    }
                    _ => {}
                }
            }

            objects.push(TokenObjectInfo {
                handle,
                label,
                class_name,
                size,
            });
        }

        objects.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(objects)
    }

    pub fn export_object(&self, handle: ObjectHandle, output_path: &Path) -> Result<(), ServiceError> {
        let attrs = self.session.get_attributes(handle, &[AttributeType::Value])?;
        let value = attrs
            .into_iter()
            .find_map(|attr| match attr {
                Attribute::Value(bytes) => Some(bytes),
                _ => None,
            })
            .ok_or_else(|| ServiceError::Message("Selected object has no readable value".into()))?;
        std::fs::write(output_path, value)?;
        Ok(())
    }

    pub fn logout(&self) {
        let _ = self.session.logout();
    }
}

impl TokenSession {
    fn collect_object_handles(&self, template: &[Attribute]) -> Result<Vec<ObjectHandle>, ServiceError> {
        let mut handles = Vec::new();
        for object in self.session.iter_objects(template)? {
            handles.push(object?);
        }
        Ok(handles)
    }
}

impl Drop for TokenSession {
    fn drop(&mut self) {
        let _ = self.session.logout();
    }
}

pub fn default_module_path() -> String {
    #[cfg(target_os = "windows")]
    {
        let candidates = [
            r"C:\Windows\System32\rtpkcs11ecp.dll",
            r"C:\Program Files\Aktiv Co\Rutoken PKCS11\rtpkcs11ecp.dll",
        ];
        return first_existing(candidates.iter().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(candidates[0]))
            .display()
            .to_string();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let candidates = [
            "/usr/lib/librtpkcs11ecp.so",
            "/usr/lib64/librtpkcs11ecp.so",
            "/usr/local/lib/librtpkcs11ecp.so",
        ];
        first_existing(candidates.iter().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(candidates[0]))
            .display()
            .to_string()
    }
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|path| path.exists())
}
