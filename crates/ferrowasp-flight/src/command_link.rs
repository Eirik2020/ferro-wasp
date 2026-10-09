//! Which command line a command came in on, so its answer goes back there.
//!
//! USB always carries the command line. A board may also bind a UART to the
//! `configurator` function, for a host on a cable or a Bluetooth module. The
//! storage owner reads both command queues and answers each command on the
//! link it came from. A flash operation that finishes later, an erase or a
//! save, answers the link that started it.

use crate::flash_storage::{CommandConsumer, ResponseFrame, ResponseProducer, StorageCommand};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandLink {
    Usb,
    Serial,
}

impl StorageCommand {
    /// Commands only the USB cable may give. They move motors or erase
    /// flights no host has stored, and a serial link may be a Bluetooth
    /// module anyone in range can pair with.
    pub const fn usb_only(&self) -> bool {
        matches!(
            self,
            Self::Motor(_) | Self::LogsEraseConfirmed | Self::FlashTestConfirmed
        )
    }
}

/// The storage owner's command queues, read in turn so a busy link cannot
/// starve the other.
pub struct CommandSources {
    usb: CommandConsumer,
    serial: Option<CommandConsumer>,
    serial_first: bool,
}

impl CommandSources {
    pub const fn new(usb: CommandConsumer, serial: Option<CommandConsumer>) -> Self {
        Self {
            usb,
            serial,
            serial_first: false,
        }
    }

    pub fn take_next(&mut self) -> Option<(StorageCommand, CommandLink)> {
        let usb = |sources: &mut Self| {
            sources
                .usb
                .dequeue()
                .map(|command| (command, CommandLink::Usb))
        };
        let serial = |sources: &mut Self| {
            sources
                .serial
                .as_mut()?
                .dequeue()
                .map(|command| (command, CommandLink::Serial))
        };
        let next = if self.serial_first {
            serial(self).or_else(|| usb(self))
        } else {
            usb(self).or_else(|| serial(self))
        };
        if let Some((_, link)) = next {
            self.serial_first = link == CommandLink::Usb;
        }
        next
    }
}

/// The storage owner's response queues, and which link the next answer is
/// for.
pub struct ResponseRouter {
    usb: ResponseProducer,
    serial: Option<ResponseProducer>,
    reply_to: CommandLink,
    maintenance: CommandLink,
}

impl ResponseRouter {
    pub const fn new(usb: ResponseProducer, serial: Option<ResponseProducer>) -> Self {
        Self {
            usb,
            serial,
            reply_to: CommandLink::Usb,
            maintenance: CommandLink::Usb,
        }
    }

    /// Answer the command that just came in on `link`.
    pub fn answer(&mut self, link: CommandLink) {
        self.reply_to = link;
    }

    /// The command being answered started a flash operation; its outcome
    /// goes to the same link.
    pub fn start_maintenance(&mut self) {
        self.maintenance = self.reply_to;
    }

    /// Answer whoever started the running flash operation.
    pub fn answer_maintenance(&mut self) {
        self.reply_to = self.maintenance;
    }

    pub const fn replying_to(&self) -> CommandLink {
        self.reply_to
    }

    /// Queue one line for the current link. `false` when its queue is full or
    /// the link does not exist.
    pub fn send(&mut self, frame: ResponseFrame) -> bool {
        let producer = match self.reply_to {
            CommandLink::Usb => &mut self.usb,
            CommandLink::Serial => match self.serial.as_mut() {
                Some(producer) => producer,
                None => return false,
            },
        };
        producer.enqueue(frame).is_ok()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::flash_storage::{CommandQueue, ResponseConsumer, ResponseQueue};
    use std::boxed::Box;

    fn line(text: &str) -> ResponseFrame {
        ResponseFrame::from_text(text).unwrap()
    }

    fn router() -> (ResponseRouter, ResponseConsumer, ResponseConsumer) {
        let (usb, usb_out) = Box::leak(Box::new(ResponseQueue::new())).split();
        let (serial, serial_out) = Box::leak(Box::new(ResponseQueue::new())).split();
        (ResponseRouter::new(usb, Some(serial)), usb_out, serial_out)
    }

    #[test]
    fn an_answer_and_a_later_outcome_go_to_the_link_that_asked() {
        let (mut router, mut usb, mut serial) = router();
        router.answer(CommandLink::Serial);
        router.start_maintenance();
        assert!(router.send(line("OK log erase started\r\n")));
        router.answer(CommandLink::Usb);
        assert!(router.send(line("OK u=0\r\n")));
        router.answer_maintenance();
        assert!(router.send(line("OK logs erased\r\n")));

        assert_eq!(usb.dequeue().unwrap().as_bytes(), b"OK u=0\r\n");
        assert!(usb.dequeue().is_none());
        assert_eq!(
            serial.dequeue().unwrap().as_bytes(),
            b"OK log erase started\r\n"
        );
        assert_eq!(serial.dequeue().unwrap().as_bytes(), b"OK logs erased\r\n");
    }

    #[test]
    fn a_board_without_a_serial_link_cannot_answer_one() {
        let (usb, _usb_out) = Box::leak(Box::new(ResponseQueue::new())).split();
        let mut router = ResponseRouter::new(usb, None);
        router.answer(CommandLink::Serial);
        assert!(!router.send(line("OK\r\n")));
    }

    #[test]
    fn sources_take_turns_so_neither_link_starves() {
        let (mut usb_in, usb) = Box::leak(Box::new(CommandQueue::new())).split();
        let (mut serial_in, serial) = Box::leak(Box::new(CommandQueue::new())).split();
        let mut sources = CommandSources::new(usb, Some(serial));
        for _ in 0..2 {
            usb_in.enqueue(StorageCommand::LogsList).unwrap();
            serial_in.enqueue(StorageCommand::LogsUnsynced).unwrap();
        }
        let links: std::vec::Vec<_> = core::iter::from_fn(|| sources.take_next())
            .map(|(_, link)| link)
            .collect();
        assert_eq!(
            links,
            [
                CommandLink::Usb,
                CommandLink::Serial,
                CommandLink::Usb,
                CommandLink::Serial
            ]
        );
    }

    #[test]
    fn motors_and_unconditional_erase_stay_on_usb() {
        use ferrowasp_core::safety::BenchMotorRequest;

        assert!(StorageCommand::Motor(BenchMotorRequest::Stop).usb_only());
        assert!(StorageCommand::LogsEraseConfirmed.usb_only());
        assert!(!StorageCommand::LogsEraseSyncedConfirmed.usb_only());
        assert!(
            !StorageCommand::LogsAck {
                flight: 1,
                pages: 1
            }
            .usb_only()
        );
        assert!(!StorageCommand::ConfigSave.usb_only());
    }
}
