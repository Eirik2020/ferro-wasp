#[task(binds = {{UART_INTERRUPT}}, priority = {{UART_IRQ_PRIORITY}}, shared = [usart1_rx_dma, osd_serial])]
fn usart1_rx_idle(cx: usart1_rx_idle::Context) {
    let delivered = (cx.shared.usart1_rx_dma, cx.shared.osd_serial)
        .lock(|rx, endpoint| rx.service_idle(endpoint));
    if delivered {
        osd_displayport::spawn().ok();
    }
}

#[task(binds = {{RX_DMA_INTERRUPT}}, priority = {{RX_DMA_IRQ_PRIORITY}}, shared = [usart1_rx_dma, osd_serial])]
fn usart1_rx_dma(cx: usart1_rx_dma::Context) {
    let delivered = (cx.shared.usart1_rx_dma, cx.shared.osd_serial)
        .lock(|rx, endpoint| rx.service_dma(endpoint));
    if delivered {
        osd_displayport::spawn().ok();
    }
}

#[task(binds = {{TX_DMA_INTERRUPT}}, priority = {{TX_DMA_IRQ_PRIORITY}}, shared = [usart1_tx_dma, osd_serial])]
fn usart1_tx_dma(cx: usart1_tx_dma::Context) {
    (cx.shared.usart1_tx_dma, cx.shared.osd_serial)
        .lock(|tx, endpoint| tx.service_irq(endpoint));
}

#[task(priority = {{REFRESH_TASK_PRIORITY}}, shared = [osd_serial])]
async fn osd_refresh_tick(mut cx: osd_refresh_tick::Context) {
    loop {
        Mono::delay({{REFRESH_PERIOD_MS}}.millis()).await;
        cx.shared.osd_serial.lock(|endpoint| endpoint.request_refresh());
        osd_displayport::spawn().ok();
    }
}

#[task(priority = {{TX_WORKER_PRIORITY}}, shared = [usart1_tx_dma, osd_serial])]
async fn usart1_tx_kick(cx: usart1_tx_kick::Context) {
    (cx.shared.usart1_tx_dma, cx.shared.osd_serial)
        .lock(|tx, endpoint| tx.start_next(endpoint));
}

#[task(priority = {{OSD_TASK_PRIORITY}}, shared = [osd_serial, osd_telemetry], local = [osd_component, osd_output])]
async fn osd_displayport(cx: osd_displayport::Context) {
    let queued = (cx.shared.osd_serial, cx.shared.osd_telemetry).lock(|endpoint, telemetry| {
        cx.local
            .osd_component
            .process(endpoint, telemetry, cx.local.osd_output)
    });
    if queued {
        usart1_tx_kick::spawn().ok();
    }
}
