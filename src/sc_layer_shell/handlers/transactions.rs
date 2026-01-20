use wayland_backend::server::ClientId;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, Resource};

use super::super::protocol::gen::sc_transaction_v1::{self, ScTransactionV1};
use crate::{sc_layer_shell::handlers::commit_transaction, state::Backend, ScreenComposer};

impl<BackendData: Backend> Dispatch<ScTransactionV1, ()> for ScreenComposer<BackendData> {
    fn request(
        state: &mut Self,
        _client: &Client,
        transaction: &ScTransactionV1,
        request: sc_transaction_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let txn_id = transaction.id();

        match request {
            sc_transaction_v1::Request::SetDuration { duration } => {
                if let Some(txn) = state.sc_transactions.get_mut(&txn_id) {
                    // Convert from seconds (f64) to milliseconds
                    txn.duration_ms = Some((duration * 1000.0) as f32);
                }
            }

            sc_transaction_v1::Request::SetDelay { delay } => {
                if let Some(txn) = state.sc_transactions.get_mut(&txn_id) {
                    // Convert from seconds (f64) to milliseconds
                    txn.delay_ms = Some((delay * 1000.0) as f32);
                }
            }

            sc_transaction_v1::Request::SetTimingFunction { timing: _ } => {
                // Timing function objects not yet implemented
                tracing::debug!("SetTimingFunction called - not yet implemented");
            }

            sc_transaction_v1::Request::EnableCompletionEvent => {
                if let Some(txn) = state.sc_transactions.get_mut(&txn_id) {
                    txn.send_completion = true;
                }
            }

            sc_transaction_v1::Request::Commit => {
                commit_transaction(state, txn_id);
            }

            sc_transaction_v1::Request::Destroy => {
                // If destroyed before commit, discard all pending changes
                state.sc_transactions.remove(&txn_id);
            }
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, transaction: &ScTransactionV1, _data: &()) {
        // Clean up transaction if still present
        state.sc_transactions.remove(&transaction.id());
    }
}
