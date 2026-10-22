//! Serial chunks in and out, and the continuity metadata that lets a consumer
//! tell lost bytes from a quiet line (ADR-0011).
//!
//! Requirements: IO-001
//! Safety-critical.

mod chunk;
mod discontinuity;
mod fault;
mod tx_chunk;

pub use chunk::{RxChunk, RxChunkError, RxCompletion};
pub use discontinuity::{Discontinuity, DiscontinuityRecord, StreamGeneration};
pub use fault::SerialFault;
pub use tx_chunk::{TxChunk, TxChunkError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TimestampMicros;

    #[test]
    fn a_received_chunk_keeps_its_bytes_arrival_and_continuity() {
        let chunk = RxChunk::<8>::from_slice(
            &[0x0f, 0x00, 0x7f],
            TimestampMicros(1_250),
            RxCompletion::Idle,
            StreamGeneration(3),
            true,
        )
        .unwrap();

        assert_eq!(chunk.as_slice(), Ok(&[0x0f, 0x00, 0x7f][..]));
        assert_eq!(chunk.timestamp, TimestampMicros(1_250));
        assert_eq!(chunk.generation, StreamGeneration(3));
        assert!(chunk.uart_error_seen);
        assert!(
            !chunk.discontinuity_before,
            "only the reader marks lost bytes, never the constructor"
        );
    }

    #[test]
    fn an_empty_or_oversized_chunk_is_refused() {
        let at = TimestampMicros(0);
        let generation = StreamGeneration(0);
        assert_eq!(
            RxChunk::<4>::from_slice(&[], at, RxCompletion::Idle, generation, false),
            Err(RxChunkError::Empty)
        );
        assert_eq!(
            RxChunk::<4>::from_slice(&[0; 5], at, RxCompletion::DmaFull, generation, false),
            Err(RxChunkError::CapacityExceeded)
        );
        assert_eq!(TxChunk::<4>::from_slice(&[]), Err(TxChunkError::Empty));
        assert_eq!(
            TxChunk::<4>::from_slice(&[0; 5]),
            Err(TxChunkError::CapacityExceeded)
        );

        let mut tampered =
            RxChunk::<4>::from_slice(&[1], at, RxCompletion::Idle, generation, false).unwrap();
        tampered.len = 0;
        assert_eq!(tampered.as_slice(), Err(RxChunkError::Empty));
        tampered.len = 5;
        assert_eq!(tampered.as_slice(), Err(RxChunkError::CapacityExceeded));
    }

    #[test]
    fn a_transmitted_chunk_round_trips() {
        let chunk = TxChunk::<16>::from_slice(b"$M>").unwrap();
        assert_eq!(chunk.as_slice(), Ok(&b"$M>"[..]));
    }

    #[test]
    fn the_stream_generation_advances_and_wraps() {
        assert_eq!(StreamGeneration(7).next(), StreamGeneration(8));
        assert_eq!(StreamGeneration(u32::MAX).next(), StreamGeneration(0));
    }

    #[test]
    fn serial_faults_map_to_embedded_io_kinds() {
        use embedded_io::{Error, ErrorKind};
        assert_eq!(SerialFault::Disabled.kind(), ErrorKind::NotConnected);
        assert_eq!(SerialFault::QueueOverflow.kind(), ErrorKind::OutOfMemory);
        assert_eq!(SerialFault::Timeout.kind(), ErrorKind::TimedOut);
        assert_eq!(SerialFault::InvalidChunk.kind(), ErrorKind::InvalidData);
        assert_eq!(SerialFault::InvalidState.kind(), ErrorKind::InvalidData);
        assert_eq!(SerialFault::DmaTransfer.kind(), ErrorKind::Other);
        assert_eq!(SerialFault::UnsupportedProtocol.kind(), ErrorKind::Other);
    }
}
