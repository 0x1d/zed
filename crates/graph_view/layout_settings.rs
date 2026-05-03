//! Persisted Terraform graph layout options (Bench parity: TB/LR × dependency flow).

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::dot_layout::{
    TerraformDependencyFlow, TerraformLayoutDirection, TerraformLayoutOptions,
};

const FILE_NAME: &str = "terraform_graph_layout.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LayoutSettingsFile {
    direction: DirectionSerde,
    dependency_flow: DependencyFlowSerde,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DirectionSerde {
    Tb,
    Lr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DependencyFlowSerde {
    DependenciesAtTop,
    DependentsAtTop,
}

impl From<TerraformLayoutDirection> for DirectionSerde {
    fn from(d: TerraformLayoutDirection) -> Self {
        match d {
            TerraformLayoutDirection::Tb => DirectionSerde::Tb,
            TerraformLayoutDirection::Lr => DirectionSerde::Lr,
        }
    }
}

impl From<DirectionSerde> for TerraformLayoutDirection {
    fn from(d: DirectionSerde) -> Self {
        match d {
            DirectionSerde::Tb => TerraformLayoutDirection::Tb,
            DirectionSerde::Lr => TerraformLayoutDirection::Lr,
        }
    }
}

impl From<TerraformDependencyFlow> for DependencyFlowSerde {
    fn from(f: TerraformDependencyFlow) -> Self {
        match f {
            TerraformDependencyFlow::DependenciesAtTop => {
                DependencyFlowSerde::DependenciesAtTop
            }
            TerraformDependencyFlow::DependentsAtTop => DependencyFlowSerde::DependentsAtTop,
        }
    }
}

impl From<DependencyFlowSerde> for TerraformDependencyFlow {
    fn from(f: DependencyFlowSerde) -> Self {
        match f {
            DependencyFlowSerde::DependenciesAtTop => {
                TerraformDependencyFlow::DependenciesAtTop
            }
            DependencyFlowSerde::DependentsAtTop => TerraformDependencyFlow::DependentsAtTop,
        }
    }
}

fn settings_path() -> PathBuf {
    let base = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("zed");
    base.join(FILE_NAME)
}

pub fn load_layout_options() -> TerraformLayoutOptions {
    let path = settings_path();
    let data = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return TerraformLayoutOptions::default(),
    };
    let parsed: LayoutSettingsFile = match serde_json::from_str(&data) {
        Ok(p) => p,
        Err(_) => return TerraformLayoutOptions::default(),
    };
    TerraformLayoutOptions {
        direction: parsed.direction.into(),
        dependency_flow: parsed.dependency_flow.into(),
    }
}

pub fn save_layout_options(options: TerraformLayoutOptions) -> anyhow::Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create_dir_all {:?}", parent))?;
    }
    let file = LayoutSettingsFile {
        direction: options.direction.into(),
        dependency_flow: options.dependency_flow.into(),
    };
    let json = serde_json::to_string_pretty(&file).context("serialize layout settings")?;
    fs::write(&path, json).with_context(|| format!("write {:?}", path))?;
    Ok(())
}
