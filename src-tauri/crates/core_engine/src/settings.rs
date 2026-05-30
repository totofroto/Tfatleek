use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppSettings {
    pub preset_paths: HashMap<String, String>,
    pub excluded_folders: HashSet<String>,
}

pub struct SettingsManager {
    settings_path: PathBuf,
    pub current: Arc<RwLock<AppSettings>>,
}

impl SettingsManager {
    pub fn new(config_dir: &Path) -> Self {
        let settings_path = config_dir.join("settings.json");
        let current = if settings_path.exists() {
            let data = fs::read_to_string(&settings_path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            AppSettings::default()
        };

        Self {
            settings_path,
            current: Arc::new(RwLock::new(current)),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let settings = self.current.read().map_err(|e| e.to_string())?;
        let data = serde_json::to_string_pretty(&*settings).map_err(|e| e.to_string())?;
        fs::write(&self.settings_path, data).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_preset_path(&self, name: String, path: String) -> Result<(), String> {
        {
            let mut settings = self.current.write().map_err(|e| e.to_string())?;
            settings.preset_paths.insert(name, path);
        }
        self.save()
    }

    pub fn add_excluded_folder(&self, folder: String) -> Result<(), String> {
        {
            let mut settings = self.current.write().map_err(|e| e.to_string())?;
            settings.excluded_folders.insert(folder);
        }
        self.save()
    }

    pub fn remove_excluded_folder(&self, folder: &str) -> Result<(), String> {
        {
            let mut settings = self.current.write().map_err(|e| e.to_string())?;
            settings.excluded_folders.remove(folder);
        }
        self.save()
    }

    pub fn is_path_protected(&self, path: &str) -> bool {
        let settings = match self.current.read() {
            Ok(s) => s,
            Err(_) => return false,
        };
        
        let target_path = Path::new(path);
        for preset_path_str in settings.preset_paths.values() {
            let preset_path = Path::new(preset_path_str);
            if target_path.starts_with(preset_path) {
                return true;
            }
        }
        false
    }
}

pub struct MasterTreeIndexer {
    pub structure_cache: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl MasterTreeIndexer {
    pub fn new() -> Self {
        Self {
            structure_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn index_path(&self, root_path: &str) -> Result<(), String> {
        let mut subfolders = Vec::new();
        let path = Path::new(root_path);

        if !path.exists() || !path.is_dir() {
            return Err("Invalid root path for indexing.".to_string());
        }

        self.recursive_index(path, &mut subfolders)?;

        let mut cache = self.structure_cache.write().map_err(|e| e.to_string())?;
        cache.insert(root_path.to_string(), subfolders);

        Ok(())
    }

    fn recursive_index(&self, path: &Path, subfolders: &mut Vec<String>) -> Result<(), String> {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    if let Some(s) = p.to_str() {
                        subfolders.push(s.to_string());
                    }
                    self.recursive_index(&p, subfolders)?;
                }
            }
        }
        Ok(())
    }

    pub fn get_cached_structure(&self, root_path: &str) -> Option<Vec<String>> {
        let cache = self.structure_cache.read().ok()?;
        cache.get(root_path).cloned()
    }
}
