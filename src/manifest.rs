use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub project: ProjectManifest,
    #[serde(default)]
    pub render: RenderManifest,
    #[serde(default)]
    pub paths: PathsManifest,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectManifest {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RenderManifest {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PathsManifest {
    #[serde(default = "default_samples_path")]
    pub samples: PathBuf,
    #[serde(default)]
    pub sample_libraries: Vec<PathBuf>,
    #[serde(default = "default_renders_path")]
    pub renders: PathBuf,
    #[serde(default = "default_build_path")]
    pub build: PathBuf,
}

impl Default for RenderManifest {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            sample_rate: default_sample_rate(),
            bit_depth: default_bit_depth(),
        }
    }
}

impl Default for PathsManifest {
    fn default() -> Self {
        Self {
            samples: default_samples_path(),
            sample_libraries: Vec::new(),
            renders: default_renders_path(),
            build: default_build_path(),
        }
    }
}

pub fn load_manifest(project_root: &Path) -> Result<Manifest> {
    let path = project_root.join("malison.toml");
    if !path.exists() {
        return Ok(Manifest::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read `{}`", path.display()))?;
    let manifest = toml::from_str::<Manifest>(&text)
        .with_context(|| format!("failed to parse `{}`", path.display()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if let Some(name) = &manifest.project.name
        && name.trim().is_empty()
    {
        bail!("manifest project name cannot be empty");
    }
    if !matches!(manifest.render.backend.as_str(), "rust" | "supercollider") {
        bail!(
            "manifest render backend `{}` is not supported",
            manifest.render.backend
        );
    }
    if manifest.render.sample_rate == 0 {
        bail!("manifest render sample_rate must be positive");
    }
    if !matches!(manifest.render.bit_depth, 16 | 24 | 32) {
        bail!("manifest render bit_depth must be 16, 24, or 32");
    }
    Ok(())
}

fn default_backend() -> String {
    "rust".to_string()
}

fn default_sample_rate() -> u32 {
    48_000
}

fn default_bit_depth() -> u16 {
    24
}

fn default_samples_path() -> PathBuf {
    PathBuf::from("samples")
}

fn default_renders_path() -> PathBuf {
    PathBuf::from("renders")
}

fn default_build_path() -> PathBuf {
    PathBuf::from("build")
}
