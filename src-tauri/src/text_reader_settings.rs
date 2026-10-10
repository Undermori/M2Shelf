use crate::db::{AppResult, Database};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextReaderSettings {
    pub mode: String,
    pub theme: String,
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: f64,
    pub italic: bool,
    pub alignment: String,
    pub line_height: f64,
    pub paragraph_spacing: f64,
    pub letter_spacing: f64,
    pub word_spacing: f64,
    pub max_width: f64,
    pub horizontal_margin: f64,
    pub vertical_margin: f64,
    pub background_color: String,
    pub text_color: String,
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub sepia: f64,
    pub hue: f64,
    pub negative: bool,
}
impl Default for TextReaderSettings {
    fn default() -> Self {
        Self {
            mode: "PAGED".into(),
            theme: "APP".into(),
            font_family: "SYSTEM".into(),
            font_size: 22.0,
            font_weight: 400.0,
            italic: false,
            alignment: "left".into(),
            line_height: 1.8,
            paragraph_spacing: 0.7,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            max_width: 800.0,
            horizontal_margin: 32.0,
            vertical_margin: 24.0,
            background_color: "#f5eedf".into(),
            text_color: "#302b25".into(),
            brightness: 100.0,
            contrast: 100.0,
            saturation: 100.0,
            sepia: 0.0,
            hue: 0.0,
            negative: false,
        }
    }
}
impl TextReaderSettings {
    pub fn validate(&self) -> AppResult<()> {
        let color = |value: &str| {
            value.len() == 7
                && value.starts_with('#')
                && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
        };
        let numeric = [
            (self.font_size, 12.0, 40.0),
            (self.font_weight, 300.0, 800.0),
            (self.line_height, 1.2, 2.6),
            (self.paragraph_spacing, 0.0, 2.0),
            (self.letter_spacing, 0.0, 0.2),
            (self.word_spacing, 0.0, 0.5),
            (self.max_width, 400.0, 1200.0),
            (self.horizontal_margin, 12.0, 80.0),
            (self.vertical_margin, 12.0, 80.0),
            (self.brightness, 50.0, 150.0),
            (self.contrast, 50.0, 150.0),
            (self.saturation, 0.0, 150.0),
            (self.sepia, 0.0, 100.0),
            (self.hue, 0.0, 360.0),
        ];
        if !matches!(self.mode.as_str(), "PAGED" | "SCROLL")
            || !matches!(
                self.theme.as_str(),
                "APP" | "PAPER" | "SEPIA" | "NIGHT" | "CUSTOM"
            )
            || !matches!(self.font_family.as_str(), "SYSTEM" | "SERIF" | "MONO")
            || !matches!(
                self.alignment.as_str(),
                "left" | "center" | "right" | "justify"
            )
            || !color(&self.background_color)
            || !color(&self.text_color)
            || numeric
                .into_iter()
                .any(|(v, min, max)| !v.is_finite() || v < min || v > max)
        {
            return Err("COMIC_INVALID_SETTINGS".into());
        }
        Ok(())
    }
}
pub fn get(database: &Database) -> AppResult<TextReaderSettings> {
    database.read_snapshot(|c| {
        let value: Option<String> = c
            .query_row(
                "SELECT value FROM settings WHERE key='text_reader'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let settings: TextReaderSettings = value
            .as_deref()
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or_default();
        Ok(if settings.validate().is_ok() {
            settings
        } else {
            TextReaderSettings::default()
        })
    })
}
pub fn save(database: &Database, settings: TextReaderSettings) -> AppResult<TextReaderSettings> {
    settings.validate()?;
    database.connect()?.execute("INSERT INTO settings(key,value) VALUES('text_reader',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&settings).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preferences_persist_independently_and_invalid_saves_preserve_previous_values() {
        let temp = tempfile::tempdir().unwrap();
        let database = Database::new(temp.path().join("preferences.sqlite"));
        database.migrate().unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO settings(key,value) VALUES('theme','light')",
                [],
            )
            .unwrap();
        let preferences = TextReaderSettings {
            mode: "SCROLL".into(),
            theme: "SEPIA".into(),
            font_size: 30.0,
            ..Default::default()
        };
        save(&database, preferences.clone()).unwrap();
        assert_eq!(get(&database).unwrap().font_size, 30.0);
        let invalid = TextReaderSettings {
            background_color: "url(https://example.invalid)".into(),
            ..preferences
        };
        assert!(save(&database, invalid).is_err());
        assert_eq!(get(&database).unwrap().theme, "SEPIA");
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row("SELECT value FROM settings WHERE key='theme'", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "light"
        );
        database
            .connect()
            .unwrap()
            .execute(
                "UPDATE settings SET value='invalid' WHERE key='text_reader'",
                [],
            )
            .unwrap();
        assert_eq!(get(&database).unwrap().font_size, 22.0);
    }
}
