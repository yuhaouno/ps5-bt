// SPDX-License-Identifier: GPL-3.0-or-later
//! One bounded send through an externally established HCI ACL credit owner.

use bt_core::{BluetoothTransport, Error as TransportError};
use bt_hci::flow::{AclCreditOwner, CreditContext, CreditError, CreditFailure};
use bt_l2cap::{Connection, FragmentError, TerminalFailure};

use crate::{DroppedPacket, Error, MediaSession, SendProgress};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmitOutcome {
    /// No native submission occurred because the owned reservation is exhausted.
    NoCredits,
    /// The backend rejected the complete fragment without accepting bytes.
    Busy,
    Accepted(SendProgress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmitError {
    Session(Error),
    /// Bind only the exact same connection epoch and proven ACL data maximum.
    OwnerMismatch,
    Credit(CreditError),
    /// A terminal credit/transport failure consumed the media cancellation.
    /// Counts include definitely accepted prior fragments. The credit failure
    /// separately states whether the current native send's outcome is uncertain.
    Terminal {
        failure: CreditFailure,
        dropped: Option<DroppedPacket>,
    },
    /// Defensive terminal path: native acceptance already occurred and must
    /// never be retried or refunded if internal confirmation cannot advance.
    Confirmation {
        source: Error,
        failure: CreditFailure,
        dropped: Option<DroppedPacket>,
    },
}

impl MediaSession<'_> {
    /// Prepare and attempt at most one ACL submission; never poll or wait.
    ///
    /// The caller supplies the single-owner transport and a separately proven
    /// reservation. No controller-wide capacity or timer establishes ownership.
    /// Backend acceptance charges one credit before advancing this media cursor.
    /// Busy/exhaustion retain the complete PDU and the prepared fragment for a
    /// byte-identical retry. Only an owned Number Of Completed Packets event,
    /// reconciled through `credits.complete`, restores a credit.
    ///
    /// Completion metadata must already have the correct epoch from a proven
    /// ordered dispatcher. This function does not receive events or preserve
    /// native traffic on its own. The caller must process cancellation evidence
    /// and stop on terminal failure; creating fresh state is not reconciliation.
    /// Keep the same established reservation and transport throughout a partial
    /// PDU. Matching handle/epoch numbers do not authenticate their source.
    /// Do not substitute another ledger/backend or mix manual confirmation
    /// with this method to bypass acceptance and credit accounting.
    pub fn transmit_next<T: BluetoothTransport + ?Sized>(
        &mut self,
        connection: Connection,
        credits: &mut AclCreditOwner,
        transport: &mut T,
        output: &mut [u8],
    ) -> Result<TransmitOutcome, TransmitError> {
        self.require_active().map_err(TransmitError::Session)?;
        self.check_connection(connection)
            .map_err(TransmitError::Session)?;
        let context = CreditContext {
            handle: connection.handle,
            epoch: connection.epoch,
        };
        let snapshot = credits.snapshot();
        if snapshot.context != context || credits.acl_data_max() != self.fragmenter.acl_data_max() {
            return Err(TransmitError::OwnerMismatch);
        }
        if let Some(failure) = snapshot.failure {
            return self.fail_transmission(connection, failure);
        }
        if self.queued().is_none() {
            return Err(TransmitError::Session(Error::Fragment(FragmentError::Idle)));
        }
        if snapshot.available == 0 {
            return Ok(TransmitOutcome::NoCredits);
        }
        let prepared = self
            .prepare_acl(connection, output)
            .map_err(TransmitError::Session)?;
        match credits.send_acl(context, &output[..prepared.packet_len], transport) {
            Ok(()) => match self.confirm_acl_accepted(connection) {
                Ok(progress) => Ok(TransmitOutcome::Accepted(progress)),
                Err(source) => {
                    let failure = credits.mark_integrity_lost();
                    let dropped = self
                        .fail_transport(connection, TerminalFailure::CreditsUnreconciled)
                        .map_err(TransmitError::Session)?;
                    Err(TransmitError::Confirmation {
                        source,
                        failure,
                        dropped,
                    })
                }
            },
            Err(CreditError::NoCredits) => Ok(TransmitOutcome::NoCredits),
            Err(CreditError::Rejected(TransportError::Busy)) => Ok(TransmitOutcome::Busy),
            Err(CreditError::Failed(failure)) => self.fail_transmission(connection, failure),
            Err(error) => Err(TransmitError::Credit(error)),
        }
    }

    fn fail_transmission(
        &mut self,
        connection: Connection,
        failure: CreditFailure,
    ) -> Result<TransmitOutcome, TransmitError> {
        let dropped = self
            .fail_transport(connection, TerminalFailure::CreditsUnreconciled)
            .map_err(TransmitError::Session)?;
        Err(TransmitError::Terminal { failure, dropped })
    }
}
