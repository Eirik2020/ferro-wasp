let dma2 = StreamsTuple::new(cx.device.DMA2, &mut rcc);
let serial: Serial<USART1, u8> = Serial::new(
    cx.device.USART1,
    (
        gpioa.pa9.into_alternate::<7>(),
        gpioa.pa10.into_alternate::<7>(),
    ),
    SerialConfig::default()
        .baudrate({{BAUD}}.bps())
        .dma(serial::config::DmaConfig::TxRx),
    &mut rcc,
)
.unwrap();
let (tx, mut rx) = serial.split();
rx.listen_idle();
let usart1_rx_dma = Usart1RxDma::new(dma2.5, rx, cx.local.rx_active, cx.local.rx_spare);
let usart1_tx_dma = Usart1TxDma::new(dma2.7, tx, cx.local.tx_buffer);
let (osd_work_tx, osd_work_rx) =
    make_channel!(OsdWork<{{BUFFER_SIZE}}>, {{RX_QUEUE_CAPACITY}});
let osd_work_idle_tx = osd_work_tx.clone();
let osd_work_dma_tx = osd_work_tx.clone();
let (osd_tx_tx, osd_tx_rx) =
    make_channel!(SerialChunk<{{BUFFER_SIZE}}>, {{TX_QUEUE_CAPACITY}});
let (tx_completion_tx, tx_completion_rx) = make_channel!(TxCompletion, 1);
osd_displayport::spawn(osd_work_rx, osd_tx_tx)
    .unwrap_or_else(|_| panic!("failed to start OSD consumer"));
usart1_tx_worker::spawn(osd_tx_rx, tx_completion_rx)
    .unwrap_or_else(|_| panic!("failed to start USART1 TX worker"));
osd_refresh_tick::spawn(osd_work_tx)
    .unwrap_or_else(|_| panic!("failed to start OSD refresh task"));
