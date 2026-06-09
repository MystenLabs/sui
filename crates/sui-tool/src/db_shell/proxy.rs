// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! HTTP proxy backend for the db-shell.
//! Delegates all operations to the running sui-node admin API.

use anyhow::{Context, bail};
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::db_shell::{
    backend::{Backend, DirEntry},
    vfs::VfsPath,
};

pub struct ProxyBackend {
    client: Client,
    base_url: String,
}

impl ProxyBackend {
    pub fn new(admin_url: &str) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url: admin_url.trim_end_matches('/').to_string(),
        })
    }

    fn ls_impl(&self, path: &VfsPath, limit: usize, cursor: bool) -> anyhow::Result<Vec<DirEntry>> {
        #[derive(Deserialize)]
        struct Entry {
            name: String,
            is_dir: bool,
        }

        let url = format!("{}/db-shell/ls", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("path", path.to_string()),
                ("limit", limit.to_string()),
                ("cursor", cursor.to_string()),
            ])
            .send()
            .context("ls request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("ls failed ({status}): {body}");
        }

        let entries: Vec<Entry> = resp.json().context("failed to parse ls response")?;
        Ok(entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                is_dir: e.is_dir,
            })
            .collect())
    }

    fn read_impl(&self, path: &VfsPath, format: &str) -> anyhow::Result<Vec<u8>> {
        let url = format!("{}/db-shell/read", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("path", path.to_string()), ("format", format.to_string())])
            .send()
            .context("read request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("read failed ({status}): {body}");
        }

        Ok(resp
            .bytes()
            .context("failed to read response body")?
            .to_vec())
    }
}

impl Backend for ProxyBackend {
    fn ls_children(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>> {
        self.ls_impl(path, limit, false)
    }

    fn ls_cursor(&self, path: &VfsPath, limit: usize) -> anyhow::Result<Vec<DirEntry>> {
        self.ls_impl(path, limit, true)
    }

    fn read_json(&self, path: &VfsPath) -> anyhow::Result<serde_json::Value> {
        let bytes = self.read_impl(path, "json")?;
        serde_json::from_slice(&bytes).context("failed to parse JSON response")
    }

    fn read_debug(&self, path: &VfsPath) -> anyhow::Result<String> {
        let bytes = self.read_impl(path, "debug")?;
        String::from_utf8(bytes).context("debug response is not valid UTF-8")
    }

    fn read_bcs(&self, path: &VfsPath) -> anyhow::Result<Vec<u8>> {
        let bytes = self.read_impl(path, "raw-bcs")?;
        Ok(bytes)
    }

    fn delete(&self, path: &VfsPath) -> anyhow::Result<()> {
        let url = format!("{}/db-shell/delete", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .query(&[("path", path.to_string())])
            .send()
            .context("delete request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("delete failed ({status}): {body}");
        }
        Ok(())
    }
}
