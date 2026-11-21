use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{settings, Result, Store, StoreError};

const APPEARANCE: &str = "appearance_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    Default,
    ScribularV1,
    Sage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Selection {
    Default,
    ScribularV1,
    Sage,
    MyTheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Automatic,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Accent {
    System,
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Graphite,
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Tint {
    None,
    Warm,
    Cool,
    Sage,
    Rose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Background {
    None,
    Stars,
    Dawn,
    SeaGlass,
    Linen,
    Owned(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThemeValues {
    pub mode: Mode,
    pub accent: Accent,
    pub tint: Tint,
    pub background: Background,
    pub blur: u8,
    pub brightness: u8,
    pub stars_motion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MyTheme {
    pub origin: Preset,
    pub values: ThemeValues,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub version: u8,
    pub selected: Selection,
    pub mode: Mode,
    pub my_theme: Option<MyTheme>,
}

pub type ThemeState = Appearance;

impl Default for Appearance {
    fn default() -> Self {
        Self {
            version: 1,
            selected: Selection::Default,
            mode: Mode::Automatic,
            my_theme: None,
        }
    }
}

impl Appearance {
    fn valid(&self) -> bool {
        if self.version != 1 || (self.selected == Selection::MyTheme && self.my_theme.is_none()) {
            return false;
        }
        self.my_theme.as_ref().is_none_or(|theme| {
            let values = &theme.values;
            let accent_valid = match &values.accent {
                Accent::Custom(color) => valid_color(color),
                _ => true,
            };
            let background_valid = match &values.background {
                Background::Owned(id) => valid_owned_id(id),
                _ => true,
            };
            accent_valid && background_valid && values.blur <= 100 && values.brightness <= 100
        })
    }
}

fn valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_owned_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl Store {
    pub fn appearance(&self) -> Result<ThemeState> {
        self.with(|c| {
            let saved = settings::select(c, APPEARANCE)?;
            Ok(saved
                .and_then(|json| serde_json::from_str::<Appearance>(&json).ok())
                .filter(Appearance::valid)
                .unwrap_or_default())
        })
    }

    pub fn set_appearance(&self, appearance: &Appearance) -> Result<ThemeState> {
        if !appearance.valid() {
            return Err(StoreError::InvalidAppearance);
        }
        self.with(|c| {
            let tx = c.transaction()?;
            settings::insert(&tx, APPEARANCE, &serde_json::to_string(appearance)?)?;
            tx.commit()?;
            Ok(appearance.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::key::MemoryKeyStore;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn open() -> (Store, std::path::PathBuf, MemoryKeyStore) {
        let path = std::env::temp_dir().join(format!(
            "scribular-appearance-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let keys = MemoryKeyStore::default();
        (Store::open(&path, &keys).unwrap(), path, keys)
    }

    fn custom() -> Appearance {
        Appearance {
            selected: Selection::MyTheme,
            my_theme: Some(MyTheme {
                origin: Preset::Sage,
                values: ThemeValues {
                    mode: Mode::Dark,
                    accent: Accent::Custom("#12abEF".into()),
                    tint: Tint::Rose,
                    background: Background::Owned("picture-1".into()),
                    blur: 25,
                    brightness: 70,
                    stars_motion: false,
                },
            }),
            ..Appearance::default()
        }
    }

    #[test]
    fn fresh_legacy_and_unknown_values_fall_back_to_default() {
        let (store, path, _) = open();
        assert_eq!(store.appearance().unwrap(), Appearance::default());
        store
            .set_setting(settings::SHOW_BACKGROUND_ANIMATION, "false")
            .unwrap();
        assert_eq!(store.appearance().unwrap(), Appearance::default());
        store
            .set_setting(APPEARANCE, r#"{"version":1,"selected":"future"}"#)
            .unwrap();
        assert_eq!(store.appearance().unwrap(), Appearance::default());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saves_atomically_and_rejects_invalid_values() {
        let (store, path, keys) = open();
        let valid = custom();
        store.set_appearance(&valid).unwrap();
        assert_eq!(store.appearance().unwrap(), valid);
        let invalid_enum = serde_json::to_string(&valid)
            .unwrap()
            .replace("\"myTheme\"", "\"future\"");
        assert!(serde_json::from_str::<Appearance>(&invalid_enum).is_err());
        for invalid in [
            {
                let mut s = valid.clone();
                s.version = 2;
                s
            },
            {
                let mut s = valid.clone();
                s.my_theme.as_mut().unwrap().values.accent = Accent::Custom("not-color".into());
                s
            },
            {
                let mut s = valid.clone();
                s.my_theme.as_mut().unwrap().values.blur = 101;
                s
            },
            {
                let mut s = valid.clone();
                s.my_theme.as_mut().unwrap().values.background =
                    Background::Owned("../secret".into());
                s
            },
        ] {
            assert!(store.set_appearance(&invalid).is_err());
            assert_eq!(store.appearance().unwrap(), valid);
        }
        store.with(|c| {
            c.execute_batch(&format!("CREATE TEMP TRIGGER reject_appearance BEFORE UPDATE ON settings WHEN NEW.key = '{APPEARANCE}' BEGIN SELECT RAISE(ABORT, 'test write failure'); END;"))?;
            Ok(())
        }).unwrap();
        assert!(store.set_appearance(&Appearance::default()).is_err());
        assert_eq!(store.appearance().unwrap(), valid);
        drop(store);
        let reopened = Store::open(&path, &keys).unwrap();
        assert_eq!(reopened.appearance().unwrap(), valid);
        drop(reopened);
        let _ = std::fs::remove_file(path);
    }
}
