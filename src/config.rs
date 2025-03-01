use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use toml;
use tracing::{debug, info};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Config {}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct UIConfig {}

pub struct NetworkConfig {}

pub struct ConnectionConfig {}

pub struct ControllerConfig {}

pub struct SavedMessages {}
