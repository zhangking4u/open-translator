use std::future::Future;
use std::pin::Pin;

use crate::domain::translation::{TranslationRequest, TranslationResult};

pub mod mock;

pub type TranslationFuture<'a> =
    Pin<Box<dyn Future<Output = TranslationResult> + Send + 'a>>;

pub trait TranslationEngine: Send + Sync {
    fn translate(&self, request: TranslationRequest) -> TranslationFuture<'_>;
}
