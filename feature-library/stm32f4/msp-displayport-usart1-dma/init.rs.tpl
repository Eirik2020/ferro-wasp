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
let osd_serial = SerialRxTx::new();
osd_refresh_tick::spawn().ok();
