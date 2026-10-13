pub mod eval;
pub mod index;
pub mod matching;
pub mod metrics;
pub mod regions;
pub mod resolver;
pub mod service;

pub mod proto {
    connectrpc::include_generated!();
}
