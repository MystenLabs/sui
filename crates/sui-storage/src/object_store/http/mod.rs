// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gcs;
mod local;
mod s3;

use std::sync::Arc;

use crate::object_store::http::gcs::GoogleCloudStorage;
use crate::object_store::http::local::LocalStorage;
use crate::object_store::http::s3::AmazonS3;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};

use crate::object_store::ObjectStoreGetExt;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use futures::{StreamExt, TryStreamExt};
use object_store::path::Path;
use object_store::{Error, GetResult, GetResultPayload, ObjectMeta};
use reqwest::header::{HeaderMap, CONTENT_LENGTH, ETAG, LAST_MODIFIED};
use reqwest::{Client, Method};

// http://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html
//
// Do not URI-encode any of the unreserved characters that RFC 3986 defines:
// A-Z, a-z, 0-9, hyphen ( - ), underscore ( _ ), period ( . ), and tilde ( ~ ).
pub(crate) const STRICT_ENCODE_SET: percent_encoding::AsciiSet = percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const STRICT_PATH_ENCODE_SET: percent_encoding::AsciiSet = STRICT_ENCODE_SET.remove(b'/');
static DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

pub trait HttpDownloaderBuilder {
    fn make_http(&self) -> Result<Arc<dyn ObjectStoreGetExt>>;
}

impl HttpDownloaderBuilder for ObjectStoreConfig {
    fn make_http(&self) -> Result<Arc<dyn ObjectStoreGetExt>> {
        match self.object_store {
            Some(ObjectStoreType::File) => {
                Ok(LocalStorage::new(self.directory.as_ref().unwrap()).map(Arc::new)?)
            }
            Some(ObjectStoreType::S3) => {
                let bucket_endpoint = if let Some(endpoint) = &self.aws_endpoint {
                    if self.aws_virtual_hosted_style_request {
                        endpoint.clone()
                    } else {
                        let bucket = self.bucket.as_ref().unwrap();
                        format!("{endpoint}/{bucket}")
                    }
                } else {
                    let bucket = self.bucket.as_ref().unwrap();
                    let region = self.aws_region.as_ref().unwrap();
                    if self.aws_virtual_hosted_style_request {
                        format!("https://{bucket}.s3.{region}.amazonaws.com")
                    } else {
                        format!("https://s3.{region}.amazonaws.com/{bucket}")
                    }
                };
                Ok(AmazonS3::new(&bucket_endpoint).map(Arc::new)?)
            }
            Some(ObjectStoreType::GCS) => {
                Ok(GoogleCloudStorage::new(self.bucket.as_ref().unwrap()).map(Arc::new)?)
            }
            _ => Err(anyhow!("At least one storage backend should be provided")),
        }
    }
}

async fn get(
    url: &str,
    store: &'static str,
    location: &Path,
    client: &Client,
) -> Result<GetResult> {
    let request = client.request(Method::GET, url);
    let response = request.send().await.context("failed to get")?;
    let meta = header_meta(location, response.headers()).context("Failed to get header")?;
    let stream = response
        .bytes_stream()
        .map_err(|source| Error::Generic {
            store,
            source: Box::new(source),
        })
        .boxed();
    Ok(GetResult {
        range: 0..meta.size,
        payload: GetResultPayload::Stream(stream),
        meta,
        attributes: object_store::Attributes::new(),
    })
}

fn header_meta(location: &Path, headers: &HeaderMap) -> Result<ObjectMeta> {
    let last_modified = headers
        .get(LAST_MODIFIED)
        .context("Missing last modified")?;

    let content_length = headers
        .get(CONTENT_LENGTH)
        .context("Missing content length")?;

    let last_modified = last_modified.to_str().context("bad header")?;
    let last_modified = DateTime::parse_from_rfc2822(last_modified)
        .context("invalid last modified")?
        .with_timezone(&Utc);

    let content_length = content_length.to_str().context("bad header")?;
    let content_length = content_length.parse().context("invalid content length")?;

    let e_tag = headers.get(ETAG).context("missing etag")?;
    let e_tag = e_tag.to_str().context("bad header")?;

    Ok(ObjectMeta {
        location: location.clone(),
        last_modified,
        size: content_length,
        e_tag: Some(e_tag.to_string()),
        version: None,
    })
}

#[cfg(test)]
mod tests {
    use crate::object_store::http::HttpDownloaderBuilder;
    use object_store::path::Path;
    use std::fs;
    use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
    use tempfile::TempDir;

    #[tokio::test]
    pub async fn test_local_download() -> anyhow::Result<()> {
        let input = TempDir::new()?;
        let input_path = input.path();
        let child = input_path.join("child");
        fs::create_dir(&child)?;
        let file1 = child.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let grandchild = child.join("grand_child");
        fs::create_dir(&grandchild)?;
        let file2 = grandchild.join("file2");
        fs::write(file2, b"Lorem ipsum")?;

        let input_store = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        }
        .make_http()?;

        let downloaded = input_store.get_bytes(&Path::from("child/file1")).await?;
        assert_eq!(downloaded.to_vec(), b"Lorem ipsum");
        Ok(())
    }
}
