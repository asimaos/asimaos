//! Faceprints, per user: /var/lib/asimaos-face/<uid>.json, readable only by root. They never leave
//! this computer; a factory reset deletes them with @var.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct UserFaces {
    pub version: u32,
    pub faces: Vec<StoredFace>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StoredFace {
    pub label: String,
    pub embeddings: Vec<Vec<f32>>,
}

pub struct Store(pub PathBuf);

impl Store {
    fn file(&self, uid: u32) -> PathBuf {
        self.0.join(format!("{uid}.json"))
    }

    pub fn load(&self, uid: u32) -> UserFaces {
        std::fs::read_to_string(self.file(uid)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, uid: u32, faces: &UserFaces) -> Result<()> {
        std::fs::create_dir_all(&self.0)?;
        std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700))?;
        let path = self.file(uid);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(faces)?)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Whether anyone on this computer has a face set up.
    pub fn anyone(&self) -> bool {
        std::fs::read_dir(&self.0)
            .map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "json")))
            .unwrap_or(false)
    }

    pub fn delete(&self, uid: u32) -> Result<()> {
        match std::fs::remove_file(self.file(uid)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}
