//! The speech model catalog.
//!
//! EchoLocal exposes exactly two models. The artifacts are the GGUF builds of
//! NVIDIA's Parakeet TDT 0.6B checkpoints published for transcribe.cpp. Sizes
//! and hashes are those of the exact file we download, so the UI can show the
//! real download size instead of an estimate of the upstream `.nemo` size.

use serde::{Deserialize, Serialize};

/// Identifier for one of the two supported speech models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ModelId {
    #[default]
    #[serde(rename = "parakeet-tdt-v2")]
    ParakeetTdtV2,
    #[serde(rename = "parakeet-tdt-v3")]
    ParakeetTdtV3,
}

impl ModelId {
    pub const ALL: [ModelId; 2] = [ModelId::ParakeetTdtV2, ModelId::ParakeetTdtV3];

    pub fn spec(self) -> &'static ModelSpec {
        match self {
            ModelId::ParakeetTdtV2 => &PARAKEET_TDT_V2,
            ModelId::ParakeetTdtV3 => &PARAKEET_TDT_V3,
        }
    }

    /// Stable string form, identical to the serde representation.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelId::ParakeetTdtV2 => "parakeet-tdt-v2",
            ModelId::ParakeetTdtV3 => "parakeet-tdt-v3",
        }
    }

    pub fn parse(value: &str) -> Option<ModelId> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }
}

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Everything needed to download, verify and describe one model artifact.
#[derive(Debug)]
pub struct ModelSpec {
    pub id: ModelId,
    /// Name shown in the UI.
    pub display_name: &'static str,
    /// One-line purpose shown under the name.
    pub subtitle: &'static str,
    /// Hugging Face repository hosting the artifact.
    pub repo: &'static str,
    /// Pinned repository revision, so the hash below always matches.
    pub revision: &'static str,
    pub filename: &'static str,
    /// Exact size of the downloaded artifact in bytes.
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 of the artifact.
    pub sha256: &'static str,
    /// Language hint passed to the model, or `None` to let it detect.
    pub language_hint: Option<&'static str>,
}

impl ModelSpec {
    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.filename
        )
    }
}

// Q8_0 is used for both models: it is effectively lossless for Parakeet and
// keeps the two choices comparable. Swapping the quantization only requires
// changing the filename, size and hash below (see the repo's model card for
// the other published files).
static PARAKEET_TDT_V2: ModelSpec = ModelSpec {
    id: ModelId::ParakeetTdtV2,
    display_name: "Parakeet TDT v2",
    subtitle: "For English",
    repo: "handy-computer/parakeet-tdt-0.6b-v2-gguf",
    revision: "07cee0616125a08ef619729bb47f40ef747e4bc4",
    filename: "parakeet-tdt-0.6b-v2-Q8_0.gguf",
    size_bytes: 729_574_912,
    sha256: "f0d0e99cebb6d3b83f1f7069b82b5d3c2e39a54545b0da039cb4bafd9c4e5caa",
    language_hint: None,
};

static PARAKEET_TDT_V3: ModelSpec = ModelSpec {
    id: ModelId::ParakeetTdtV3,
    display_name: "Parakeet TDT v3",
    subtitle: "For multilingual speech",
    repo: "handy-computer/parakeet-tdt-0.6b-v3-gguf",
    revision: "85ac09ea12fc4b1112fa76810059364bc6adc9de",
    filename: "parakeet-tdt-0.6b-v3-Q8_0.gguf",
    size_bytes: 739_508_576,
    sha256: "5859f77944efcd8eafa23a6350731960b2b55b2203df51f319665c807d802cc7",
    language_hint: None,
};

/// Human-readable size, e.g. `730 MB`. Uses decimal megabytes, like Finder.
pub fn format_size(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_two_models_with_expected_labels() {
        assert_eq!(ModelId::ALL.len(), 2);
        let v2 = ModelId::ParakeetTdtV2.spec();
        let v3 = ModelId::ParakeetTdtV3.spec();
        assert_eq!(
            (v2.display_name, v2.subtitle),
            ("Parakeet TDT v2", "For English")
        );
        assert_eq!(
            (v3.display_name, v3.subtitle),
            ("Parakeet TDT v3", "For multilingual speech")
        );
    }

    #[test]
    fn specs_are_self_consistent() {
        for id in ModelId::ALL {
            let spec = id.spec();
            assert_eq!(spec.id, id);
            assert_eq!(spec.sha256.len(), 64);
            assert!(spec
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(spec.filename.ends_with(".gguf"));
            assert!(spec.download_url().ends_with(spec.filename));
            assert!(spec.download_url().contains(spec.revision));
        }
    }

    #[test]
    fn id_round_trips_through_string_and_serde() {
        for id in ModelId::ALL {
            assert_eq!(ModelId::parse(id.as_str()), Some(id));
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.as_str()));
            assert_eq!(serde_json::from_str::<ModelId>(&json).unwrap(), id);
        }
        assert_eq!(ModelId::parse("whisper"), None);
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(729_574_912), "730 MB");
        assert_eq!(format_size(1_255_869_856), "1.3 GB");
    }
}
