use dirs;
use failure::{Error, Fail};
use serde_yaml;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use self::api::Config;
pub mod api;

pub const CONFIG_ENV: &str = "KUBECONFIG";

#[derive(Fail, Debug)]
#[fail(display = "Config error: {}", msg)]
pub struct ConfigError {
    msg: &'static str,
}
pub fn config_err(msg: &'static str) -> ConfigError {
    ConfigError { msg: msg }
}

pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".kube").join("config"))
}

pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config, Error> {
    let f = File::open(path)?;
    let config = serde_yaml::from_reader(f)?;
    Ok(config)
}

#[derive(Debug, Clone, Default)]
pub struct ConfigContext {
    pub user: api::AuthInfo,
    pub cluster: api::Cluster,
    pub default_namespace: Option<String>,
}

fn data_or_file(data: &[u8], file: &Path) -> Option<io::Result<Vec<u8>>> {
    let ret = if data.len() > 0 {
        Ok(data.to_vec())
    } else if file.as_os_str().len() > 0 {
        File::open(file).and_then(|f| f.bytes().collect())
    } else {
        return None;
    };
    Some(ret)
}

impl api::Cluster {
    pub fn certificate_authority_read(&self) -> Option<io::Result<Vec<u8>>> {
        data_or_file(
            &self.certificate_authority_data,
            &self.certificate_authority,
        )
    }
}

impl api::AuthInfo {
    pub fn client_certificate_read(&self) -> Option<io::Result<Vec<u8>>> {
        data_or_file(&self.client_certificate_data, &self.client_certificate)
    }

    pub fn client_key_read(&self) -> Option<io::Result<Vec<u8>>> {
        data_or_file(&self.client_key_data, &self.client_key)
    }
}

impl api::Config {
    pub fn config_context(&self, name: &str) -> Result<ConfigContext, ConfigError> {
        let ctx = self
            .contexts
            .iter()
            .find(|e| e.name == name)
            .map(|e| &e.context)
            .ok_or(config_err("context doesn't exist"))?;
        let cluster = self
            .clusters
            .iter()
            .find(|e| e.name == ctx.cluster)
            .map(|e| &e.cluster)
            .ok_or(config_err("context cluster doesn't exist"))?;
        let user = self
            .users
            .iter()
            .find(|e| e.name == ctx.user)
            .map(|e| &e.user)
            .ok_or(config_err("context user doesn't exist"))?;
        let default_namespace = &ctx.namespace;

        Ok(ConfigContext {
            user: user.clone(),
            cluster: cluster.clone(),
            default_namespace: default_namespace.clone(),
        })
    }
}
