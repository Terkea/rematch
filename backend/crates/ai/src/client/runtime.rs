//! The production [`AiClient`]: interactive calls go to the Agent SDK sidecar (ADR 0062), batch
//! calls to the Messages Batches client. A dev OAuth token never reaches Rust, so batches need
//! `ANTHROPIC_API_KEY`; without it they give `Unavailable(NoApiKey)` and `batches_enabled()` is
//! false (callers fall back to interactive calls).

use std::sync::Arc;

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};

use super::batches::{BatchesClient, BatchesConfig};
use super::port::{
    AiClient, AiError, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest, SchemaResolver,
    StructuredRequest, StructuredResult, ToolHandler, TurnEvent, UnavailableReason,
};
use super::sidecar::{SidecarClient, SidecarConfig};
use super::tool_server::ToolServer;

#[derive(Debug)]
pub struct AiRuntime {
    pub sidecar: SidecarClient,
    pub batches: Option<BatchesClient>,
}

fn no_api_key() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::NoApiKey,
        retry_after: None,
    }
}

impl AiRuntime {
    /// From `AI_SIDECAR_URL`/`AI_SIDECAR_TOKEN` and, optionally, `ANTHROPIC_API_KEY`. `Ok(None)`
    /// when the sidecar is not configured (AI features are then off).
    pub fn from_env(tools: Option<Arc<ToolServer>>) -> Result<Option<AiRuntime>, AiError> {
        let Some(cfg) = SidecarConfig::from_env() else {
            return Ok(None);
        };
        let batches = BatchesConfig::from_env()
            .map(BatchesClient::new)
            .transpose()?;
        Ok(Some(AiRuntime {
            sidecar: SidecarClient::new(cfg, tools)?,
            batches,
        }))
    }
}

impl AiClient for AiRuntime {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(self.sidecar.structured(req))
    }

    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        self.sidecar.stream_turn(req, tools)
    }

    fn batches_enabled(&self) -> bool {
        self.batches.is_some()
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        Box::pin(async move {
            match &self.batches {
                Some(b) => b.submit(items).await,
                None => Err(no_api_key()),
            }
        })
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        Box::pin(async move {
            match &self.batches {
                Some(b) => b.status(id).await,
                None => Err(no_api_key()),
            }
        })
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        match &self.batches {
            Some(b) => b.results(id, resolver),
            None => stream::once(async { Err(no_api_key()) }).boxed(),
        }
    }
}
