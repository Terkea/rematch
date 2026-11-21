//! In-memory `PlatformClient` that serves canned JSON/NDJSON and records every call.

use std::collections::HashMap;
use std::sync::Mutex;

use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use platforms::{Headers, PlatformClient, PlatformError};
use serde_json::Value;

#[derive(Default)]
pub struct FakeClient {
    /// `get_json` responses by exact URL; an unknown URL is `NotFound`.
    pub json: HashMap<String, Value>,
    /// Every `get_ndjson` call streams these values.
    pub ndjson: Vec<Value>,
    /// `(url, headers)` of every call, in order.
    pub calls: Mutex<Vec<(String, Headers)>>,
}

impl FakeClient {
    pub fn with_ndjson(ndjson: Vec<Value>) -> Self {
        Self {
            ndjson,
            ..Self::default()
        }
    }

    pub fn calls(&self) -> Vec<(String, Headers)> {
        self.calls.lock().unwrap().clone()
    }
}

impl PlatformClient for FakeClient {
    fn get_json<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxFuture<'a, Result<Value, PlatformError>> {
        self.calls.lock().unwrap().push((url.to_owned(), headers));
        let res = self
            .json
            .get(url)
            .cloned()
            .ok_or_else(|| PlatformError::NotFound {
                url: url.to_owned(),
            });
        Box::pin(async move { res })
    }

    fn get_ndjson<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxStream<'a, Result<Value, PlatformError>> {
        self.calls.lock().unwrap().push((url.to_owned(), headers));
        futures::stream::iter(self.ndjson.clone().into_iter().map(Ok)).boxed()
    }
}
