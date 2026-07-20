mod async_device;
mod fault;
mod job;
mod mailbox;
mod transaction;
mod transaction_id;

pub use async_device::{
    AsyncSpiDevice, AsyncSpiExecutor, CriticalSectionSpiExecutor, SharedSpiRequestMailbox,
    SpiDeviceError,
};
pub use fault::SpiFault;
pub use job::{OwnedSpiJob, OwnedSpiOperation, SpiJobError, SpiOperationRef};
pub use mailbox::{SpiMailboxError, SpiMailboxOwnerAction, SpiRequestMailbox};
pub use transaction::{
    SpiCompletion, SpiGeneration, SpiJobState, SpiJobToken, SpiTransactionState, SpiTransitionError,
};
pub use transaction_id::TransactionId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiDeadlineUs(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiOperation {
    Read { len: usize },
    Write { len: usize },
    Transfer { len: usize },
}
