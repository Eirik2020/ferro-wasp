#[task(
    binds = {{UART_INTERRUPT}},
    priority = {{UART_IRQ_PRIORITY}},
    shared = [{{RX_DMA_RESOURCE}}, {{OSD_FAULT_RESOURCE}}],
    local = [{{OSD_WORK_IDLE_SENDER_RESOURCE}}]
)]
fn usart1_rx_idle(mut cx: usart1_rx_idle::Context) {
    let outcome = cx.shared.{{RX_DMA_RESOURCE}}.lock(|rx| rx.service_idle());
    let fault = match outcome {
        RxServiceResult::NoData => None,
        RxServiceResult::Fault(fault) => Some(fault),
        RxServiceResult::Chunk(chunk) => {
            match cx
                .local
                .{{OSD_WORK_IDLE_SENDER_RESOURCE}}
                .try_send(OsdWork::Rx(chunk))
            {
                Ok(()) => None,
                Err(TrySendError::Full(_)) => Some(OsdFaultId::RxWorkQueueFull),
                Err(TrySendError::NoReceiver(_)) => {
                    Some(OsdFaultId::RxWorkConsumerClosed)
                }
            }
        }
    };
    if let Some(fault) = fault {
        cx.shared
            .{{OSD_FAULT_RESOURCE}}
            .lock(|faults| faults.record(fault));
    }
}

#[task(
    binds = {{RX_DMA_INTERRUPT}},
    priority = {{RX_DMA_IRQ_PRIORITY}},
    shared = [{{RX_DMA_RESOURCE}}, {{OSD_FAULT_RESOURCE}}],
    local = [{{OSD_WORK_DMA_SENDER_RESOURCE}}]
)]
fn usart1_rx_dma(mut cx: usart1_rx_dma::Context) {
    let outcome = cx.shared.{{RX_DMA_RESOURCE}}.lock(|rx| rx.service_dma());
    let fault = match outcome {
        RxServiceResult::NoData => None,
        RxServiceResult::Fault(fault) => Some(fault),
        RxServiceResult::Chunk(chunk) => {
            match cx
                .local
                .{{OSD_WORK_DMA_SENDER_RESOURCE}}
                .try_send(OsdWork::Rx(chunk))
            {
                Ok(()) => None,
                Err(TrySendError::Full(_)) => Some(OsdFaultId::RxWorkQueueFull),
                Err(TrySendError::NoReceiver(_)) => {
                    Some(OsdFaultId::RxWorkConsumerClosed)
                }
            }
        }
    };
    if let Some(fault) = fault {
        cx.shared
            .{{OSD_FAULT_RESOURCE}}
            .lock(|faults| faults.record(fault));
    }
}

#[task(
    binds = {{TX_DMA_INTERRUPT}},
    priority = {{TX_DMA_IRQ_PRIORITY}},
    shared = [{{TX_DMA_RESOURCE}}, {{OSD_FAULT_RESOURCE}}],
    local = [{{TX_COMPLETION_SENDER_RESOURCE}}]
)]
fn usart1_tx_dma(mut cx: usart1_tx_dma::Context) {
    let outcome = cx.shared.{{TX_DMA_RESOURCE}}.lock(|tx| tx.service_irq());
    if let Some(fault) = outcome.fault {
        cx.shared
            .{{OSD_FAULT_RESOURCE}}
            .lock(|faults| faults.record(fault));
    }
    if let Some(completion) = outcome.completion {
        let fault = match cx
            .local
            .{{TX_COMPLETION_SENDER_RESOURCE}}
            .try_send(completion)
        {
            Ok(()) => None,
            Err(TrySendError::Full(_)) => Some(OsdFaultId::TxCompletionQueueFull),
            Err(TrySendError::NoReceiver(_)) => {
                Some(OsdFaultId::TxCompletionConsumerClosed)
            }
        };
        if let Some(fault) = fault {
            cx.shared
                .{{OSD_FAULT_RESOURCE}}
                .lock(|faults| faults.record(fault));
        }
    }
}

#[task(priority = {{REFRESH_TASK_PRIORITY}}, shared = [{{OSD_FAULT_RESOURCE}}])]
async fn osd_refresh_tick(
    mut cx: osd_refresh_tick::Context,
    mut work_tx: Sender<'static, OsdWork<{{BUFFER_SIZE}}>, {{RX_QUEUE_CAPACITY}}>,
) {
    loop {
        // Fixed-delay periodic semantics: wait one full period after each
        // completed enqueue attempt; missed releases are not replayed.
        Mono::delay({{REFRESH_PERIOD_MS}}.millis()).await;
        let fault = match work_tx.try_send(OsdWork::Refresh) {
            Ok(()) => None,
            Err(TrySendError::Full(_)) => Some(OsdFaultId::RefreshQueueFull),
            Err(TrySendError::NoReceiver(_)) => Some(OsdFaultId::RxWorkConsumerClosed),
        };
        if let Some(fault) = fault {
            cx.shared
                .{{OSD_FAULT_RESOURCE}}
                .lock(|faults| faults.record(fault));
        }
    }
}

#[task(
    priority = {{TX_WORKER_PRIORITY}},
    shared = [{{TX_DMA_RESOURCE}}, {{OSD_FAULT_RESOURCE}}]
)]
async fn usart1_tx_worker(
    mut cx: usart1_tx_worker::Context,
    mut tx_rx: Receiver<'static, SerialChunk<{{BUFFER_SIZE}}>, {{TX_QUEUE_CAPACITY}}>,
    mut completion_rx: Receiver<'static, TxCompletion, 1>,
) {
    loop {
        let chunk = match tx_rx.recv().await {
            Ok(chunk) => chunk,
            Err(_) => {
                cx.shared
                    .{{OSD_FAULT_RESOURCE}}
                    .lock(|faults| faults.record(OsdFaultId::TxProducerClosed));
                return;
            }
        };
        let started = cx.shared.{{TX_DMA_RESOURCE}}.lock(|tx| tx.start(chunk));
        if let Err(fault) = started {
            cx.shared
                .{{OSD_FAULT_RESOURCE}}
                .lock(|faults| faults.record(fault));
            continue;
        }
        match completion_rx.recv().await {
            Ok(TxCompletion::Complete) => {}
            Ok(TxCompletion::Failed) => {
                // The IRQ records the specific DMA fault. The completion is
                // still consumed so the worker can service later frames.
            }
            Err(_) => {
                cx.shared
                    .{{OSD_FAULT_RESOURCE}}
                    .lock(|faults| {
                        faults.record(OsdFaultId::TxCompletionProducerClosed)
                    });
                return;
            }
        }
    }
}

#[task(
    priority = {{OSD_TASK_PRIORITY}},
    shared = [{{OSD_TELEMETRY_RESOURCE}}, {{OSD_FAULT_RESOURCE}}],
    local = [{{OSD_COMPONENT_RESOURCE}}, {{OSD_OUTPUT_RESOURCE}}]
)]
async fn osd_displayport(
    mut cx: osd_displayport::Context,
    mut work_rx: Receiver<'static, OsdWork<{{BUFFER_SIZE}}>, {{RX_QUEUE_CAPACITY}}>,
    mut tx_sender: Sender<'static, SerialChunk<{{BUFFER_SIZE}}>, {{TX_QUEUE_CAPACITY}}>,
) {
    loop {
        let work = match work_rx.recv().await {
            Ok(work) => work,
            Err(_) => {
                cx.shared
                    .{{OSD_FAULT_RESOURCE}}
                    .lock(|faults| faults.record(OsdFaultId::WorkProducersClosed));
                return;
            }
        };
        let telemetry = cx
            .shared
            .{{OSD_TELEMETRY_RESOURCE}}
            .lock(|telemetry| telemetry.snapshot());
        let mut full = 0_u32;
        let mut closed = 0_u32;
        let dropped = cx.local.{{OSD_COMPONENT_RESOURCE}}.process_work(
            work,
            telemetry,
            cx.local.{{OSD_OUTPUT_RESOURCE}},
            |chunk| match tx_sender.try_send(chunk) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    full = full.saturating_add(1);
                    false
                }
                Err(TrySendError::NoReceiver(_)) => {
                    closed = closed.saturating_add(1);
                    false
                }
            },
        );
        debug_assert_eq!(dropped, full.saturating_add(closed));
        if full != 0 || closed != 0 {
            cx.shared.{{OSD_FAULT_RESOURCE}}.lock(|faults| {
                faults.record_n(OsdFaultId::TxQueueFull, full);
                faults.record_n(OsdFaultId::TxConsumerClosed, closed);
            });
        }
    }
}
