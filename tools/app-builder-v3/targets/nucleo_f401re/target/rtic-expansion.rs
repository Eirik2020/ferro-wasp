#[doc = r" The RTIC application module"] pub mod app
{
    #[doc =
    r" Always include the device crate which contains the vector table"] use
    stm32f4xx_hal :: pac as
    you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml;
    #[doc =
    r" Holds the maximum priority level for use by async HAL drivers."]
    #[no_mangle] static RTIC_ASYNC_MAX_LOGICAL_PRIO : u8 = 1u8; use crate ::
    prelude :: * ; const SYSTEM_CLOCK_HZ : u32 = 84_000_000;
    systick_monotonic! (Mono, 1_000); #[doc = r" User code end"] impl < 'a >
    __rtic_internal_initLocalResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_initLocalResources
            {
                uart2_rx_buffers : & mut *
                __rtic_internal_local_init_uart2_rx_buffers.get_mut(),
                uart2_free_queue : & mut *
                __rtic_internal_local_init_uart2_free_queue.get_mut(),
                uart2_filled_queue : & mut *
                __rtic_internal_local_init_uart2_filled_queue.get_mut(),
                __rtic_internal_marker : :: core :: marker :: PhantomData,
            }
        }
    } #[doc = r"Shared resources"] struct Shared
    { blink_enabled : bool, uart2 : Uart2RxIrq, uart2_rx : UartRxParserSide, }
    #[doc = r"Local resources"] struct Local
    {
        led3 : Pin < 'A', 5, Output < PushPull > > , user_button : Pin < 'C',
        13, Input > , comport_decoder : LineConsumer,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Local resources `init` has access to"] pub struct
    __rtic_internal_initLocalResources < 'a >
    {
        #[allow(missing_docs)] pub uart2_rx_buffers : & 'static mut
        UartRxBufferBank, #[allow(missing_docs)] pub uart2_free_queue : &
        'static mut UartRxFreeQueue, #[allow(missing_docs)] pub
        uart2_filled_queue : & 'static mut UartRxFilledQueue, #[doc(hidden)]
        pub __rtic_internal_marker : :: core :: marker :: PhantomData < & 'a
        () > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct __rtic_internal_init_Context <
    'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > ,
        #[doc = r" The space used to allocate async executors in bytes."] pub
        executors_size : usize, #[doc = r" Core peripherals"] pub core : rtic
        :: export :: Peripherals, #[doc = r" Device peripherals (PAC)"] pub
        device : stm32f4xx_hal :: pac :: Peripherals,
        #[doc = r" Critical section token for init"] pub cs : rtic :: export
        :: CriticalSection < 'a > ,
        #[doc = r" Local Resources this task has access to"] pub local : init
        :: LocalResources < 'a > ,
    } impl < 'a > __rtic_internal_init_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn
        new(core : rtic :: export :: Peripherals, executors_size : usize) ->
        Self
        {
            __rtic_internal_init_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, core :
                core, device : stm32f4xx_hal :: pac :: Peripherals :: steal(),
                cs : rtic :: export :: CriticalSection :: new(),
                executors_size, local : init :: LocalResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Initialization function"] pub mod init
    {
        #[doc(inline)] pub use super :: __rtic_internal_initLocalResources as
        LocalResources; #[doc(inline)] pub use super ::
        __rtic_internal_init_Context as Context;
    } #[inline(always)] #[allow(non_snake_case)] fn init(cx : init :: Context)
    -> (Shared, Local)
    {
        let mut rcc = ferrowasp_stm32f4 :: clocks ::
        freeze_hsi(cx.device.RCC.constrain(), SYSTEM_CLOCK_HZ, false); Mono ::
        start(cx.core.SYST, SYSTEM_CLOCK_HZ); let gpioa =
        cx.device.GPIOA.split(& mut rcc); let gpioc =
        cx.device.GPIOC.split(& mut rcc); let mut syscfg =
        cx.device.SYSCFG.constrain(& mut rcc); let mut exti = cx.device.EXTI;
        let mut led3 =
        gpioa.pa5.into_push_pull_output_in_state(PinState :: Low);
        led3.set_internal_resistor(Pull :: None);
        led3.set_speed(Speed :: Low); let user_button = Input ::
        new(gpioc.pc13, Pull :: Up); let user_button = ferrowasp_stm32f4 ::
        exti ::
        init_input(user_button, & mut syscfg, & mut exti, Edge :: Falling);
        let dma1 = StreamsTuple :: new(cx.device.DMA1, & mut rcc); let
        uart2_parts = ferrowasp_stm32f4 :: uart_dma ::
        init_usart2_rx_only(Usart2RxOnlyResources
        { rx_pin : gpioa.pa3, usart : cx.device.USART2, rx_dma : dma1.5, }, &
        mut rcc, SerialProtocol :: Raw, UartRxStorageResources
        {
            buffers : cx.local.uart2_rx_buffers, free_queue :
            cx.local.uart2_free_queue, filled_queue :
            cx.local.uart2_filled_queue,
        },); let uart2 = uart2_parts.irq; let uart2_rx = uart2_parts.parser;
        blink_led ::
        spawn().expect("init must spawn declared task blink_led");
        comport_consumer ::
        spawn().expect("init must spawn declared task comport_consumer");
        (Shared { blink_enabled : true, uart2, uart2_rx : uart2_rx, }, Local
        { led3, user_button, comport_decoder : LineConsumer :: new(), })
    } #[allow(non_snake_case)] #[no_mangle] unsafe fn EXTI15_10()
    {
        const PRIORITY : u8 = 2u8; rtic :: export ::
        run(PRIORITY, || { button_exti(button_exti :: Context :: new()) });
    } impl < 'a > __rtic_internal_button_extiLocalResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_button_extiLocalResources
            {
                user_button : & mut *
                (& mut *
                __rtic_internal_local_resource_user_button.get_mut()).as_mut_ptr(),
                __rtic_internal_marker : :: core :: marker :: PhantomData,
            }
        }
    } impl < 'a > __rtic_internal_button_extiSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_button_extiSharedResources
            {
                blink_enabled : shared_resources ::
                blink_enabled_that_needs_to_be_locked :: new(),
                __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } #[allow(non_snake_case)] #[no_mangle] unsafe fn DMA1_STREAM5()
    {
        const PRIORITY : u8 = 3u8; rtic :: export ::
        run(PRIORITY, ||
        { uart2_dma_irq(uart2_dma_irq :: Context :: new()) });
    } impl < 'a > __rtic_internal_uart2_dma_irqSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_uart2_dma_irqSharedResources
            {
                uart2 : shared_resources :: uart2_that_needs_to_be_locked ::
                new(), __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } #[allow(non_snake_case)] #[no_mangle] unsafe fn USART2()
    {
        const PRIORITY : u8 = 3u8; rtic :: export ::
        run(PRIORITY, ||
        { uart2_idle_irq(uart2_idle_irq :: Context :: new()) });
    } impl < 'a > __rtic_internal_uart2_idle_irqSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_uart2_idle_irqSharedResources
            {
                uart2 : shared_resources :: uart2_that_needs_to_be_locked ::
                new(), __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Local resources `button_exti` has access to"] pub struct
    __rtic_internal_button_extiLocalResources < 'a >
    {
        #[allow(missing_docs)] pub user_button : & 'a mut Pin < 'C', 13, Input
        > , #[doc(hidden)] pub __rtic_internal_marker : :: core :: marker ::
        PhantomData < & 'a () > ,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `button_exti` has access to"] pub struct
    __rtic_internal_button_extiSharedResources < 'a >
    {
        #[allow(missing_docs)] pub blink_enabled : shared_resources ::
        blink_enabled_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_button_exti_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Local Resources this task has access to"] pub
        local : button_exti :: LocalResources < 'a > ,
        #[doc = r" Shared Resources this task has access to"] pub shared :
        button_exti :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_button_exti_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_button_exti_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, local :
                button_exti :: LocalResources :: new(), shared : button_exti
                :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Hardware task"] pub mod button_exti
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_button_extiLocalResources as LocalResources;
        #[doc(inline)] pub use super ::
        __rtic_internal_button_extiSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_button_exti_Context as
        Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `uart2_dma_irq` has access to"] pub struct
    __rtic_internal_uart2_dma_irqSharedResources < 'a >
    {
        #[allow(missing_docs)] pub uart2 : shared_resources ::
        uart2_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_uart2_dma_irq_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Shared Resources this task has access to"] pub
        shared : uart2_dma_irq :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_uart2_dma_irq_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_uart2_dma_irq_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, shared :
                uart2_dma_irq :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Hardware task"] pub mod uart2_dma_irq
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_uart2_dma_irqSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_uart2_dma_irq_Context
        as Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `uart2_idle_irq` has access to"] pub struct
    __rtic_internal_uart2_idle_irqSharedResources < 'a >
    {
        #[allow(missing_docs)] pub uart2 : shared_resources ::
        uart2_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_uart2_idle_irq_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Shared Resources this task has access to"] pub
        shared : uart2_idle_irq :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_uart2_idle_irq_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_uart2_idle_irq_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, shared :
                uart2_idle_irq :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Hardware task"] pub mod uart2_idle_irq
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_uart2_idle_irqSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_uart2_idle_irq_Context
        as Context;
    } #[allow(non_snake_case)] fn button_exti(mut cx : button_exti :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ;
        cx.local.user_button.clear_interrupt_pending_bit(); let enabled =
        cx.shared.blink_enabled.lock(| enabled |
        { * enabled = ! * enabled; * enabled }); defmt :: info!
        ("Blink enabled: {}", enabled);
    } #[allow(non_snake_case)] fn
    uart2_dma_irq(mut cx : uart2_dma_irq :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; match
        cx.shared.uart2.lock(| rx | rx.service_dma_irq())
        {
            UartRxIrqOutcome :: Ignored | UartRxIrqOutcome :: Delivered |
            UartRxIrqOutcome :: NoChunk => {} UartRxIrqOutcome :: DmaError =>
            defmt :: warn! ("UART RX DMA error"), UartRxIrqOutcome ::
            DeliveryError(_) =>
            { defmt :: warn! ("UART RX DMA buffer delivery error") }
        }
    } #[allow(non_snake_case)] fn
    uart2_idle_irq(mut cx : uart2_idle_irq :: Context)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; match
        cx.shared.uart2.lock(| rx | rx.service_idle_irq())
        {
            UartRxIrqOutcome :: Ignored | UartRxIrqOutcome :: Delivered |
            UartRxIrqOutcome :: NoChunk => {} UartRxIrqOutcome :: DmaError =>
            defmt :: warn! ("UART RX peripheral error"), UartRxIrqOutcome ::
            DeliveryError(_) =>
            { defmt :: warn! ("UART RX IDLE buffer delivery error") }
        }
    } impl < 'a > __rtic_internal_blink_ledLocalResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_blink_ledLocalResources
            {
                led3 : & mut *
                (& mut *
                __rtic_internal_local_resource_led3.get_mut()).as_mut_ptr(),
                __rtic_internal_marker : :: core :: marker :: PhantomData,
            }
        }
    } impl < 'a > __rtic_internal_blink_ledSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_blink_ledSharedResources
            {
                blink_enabled : shared_resources ::
                blink_enabled_that_needs_to_be_locked :: new(),
                __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } impl < 'a > __rtic_internal_comport_consumerLocalResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_comport_consumerLocalResources
            {
                comport_decoder : & mut *
                (& mut *
                __rtic_internal_local_resource_comport_decoder.get_mut()).as_mut_ptr(),
                __rtic_internal_marker : :: core :: marker :: PhantomData,
            }
        }
    } impl < 'a > __rtic_internal_comport_consumerSharedResources < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_comport_consumerSharedResources
            {
                uart2_rx : shared_resources ::
                uart2_rx_that_needs_to_be_locked :: new(),
                __rtic_internal_marker : core :: marker :: PhantomData,
            }
        }
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Local resources `blink_led` has access to"] pub struct
    __rtic_internal_blink_ledLocalResources < 'a >
    {
        #[allow(missing_docs)] pub led3 : & 'a mut Pin < 'A', 5, Output <
        PushPull > > , #[doc(hidden)] pub __rtic_internal_marker : :: core ::
        marker :: PhantomData < & 'a () > ,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `blink_led` has access to"] pub struct
    __rtic_internal_blink_ledSharedResources < 'a >
    {
        #[allow(missing_docs)] pub blink_enabled : shared_resources ::
        blink_enabled_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Spawns the task directly"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_blink_led_spawn() -> :: core ::
    result :: Result < (), () >
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(blink_led, & __rtic_internal_blink_led_EXEC); if
            exec.try_allocate()
            {
                exec.spawn(blink_led(unsafe
                { blink_led :: Context :: new() })); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0); Ok(())
            } else { Err(()) }
        }
    } #[doc = r" Gives waker to the task"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_blink_led_waker() -> :: core :: task
    :: Waker
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(blink_led, & __rtic_internal_blink_led_EXEC);
            exec.waker(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(blink_led, & __rtic_internal_blink_led_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0);
            })
        }
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_blink_led_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Local Resources this task has access to"] pub
        local : blink_led :: LocalResources < 'a > ,
        #[doc = r" Shared Resources this task has access to"] pub shared :
        blink_led :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_blink_led_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_blink_led_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, local :
                blink_led :: LocalResources :: new(), shared : blink_led ::
                SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Software task"] pub mod blink_led
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_blink_ledLocalResources as LocalResources;
        #[doc(inline)] pub use super ::
        __rtic_internal_blink_ledSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_blink_led_spawn as
        spawn; #[doc(inline)] pub use super :: __rtic_internal_blink_led_waker
        as waker; #[doc(inline)] pub use super ::
        __rtic_internal_blink_led_Context as Context;
    } #[doc = r" Spawns the task directly"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_report_blink_spawn(_0 : u32,) -> ::
    core :: result :: Result < (), u32 >
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_2_args(report_blink, &
            __rtic_internal_report_blink_EXEC); if exec.try_allocate()
            {
                exec.spawn(report_blink(unsafe
                { report_blink :: Context :: new() }, _0)); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0); Ok(())
            } else { Err(_0) }
        }
    } #[doc = r" Gives waker to the task"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_report_blink_waker() -> :: core ::
    task :: Waker
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_2_args(report_blink, &
            __rtic_internal_report_blink_EXEC);
            exec.waker(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_2_args(report_blink, &
                __rtic_internal_report_blink_EXEC); exec.set_pending(); rtic
                :: export :: pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0);
            })
        }
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_report_blink_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > ,
    } impl < 'a > __rtic_internal_report_blink_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_report_blink_Context
            { __rtic_internal_p : :: core :: marker :: PhantomData, }
        }
    } #[allow(non_snake_case)] #[doc = "Software task"] pub mod report_blink
    {
        #[doc(inline)] pub use super :: __rtic_internal_report_blink_spawn as
        spawn; #[doc(inline)] pub use super ::
        __rtic_internal_report_blink_waker as waker; #[doc(inline)] pub use
        super :: __rtic_internal_report_blink_Context as Context;
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Local resources `comport_consumer` has access to"] pub struct
    __rtic_internal_comport_consumerLocalResources < 'a >
    {
        #[allow(missing_docs)] pub comport_decoder : & 'a mut LineConsumer,
        #[doc(hidden)] pub __rtic_internal_marker : :: core :: marker ::
        PhantomData < & 'a () > ,
    } #[allow(non_snake_case)] #[allow(non_camel_case_types)]
    #[doc = "Shared resources `comport_consumer` has access to"] pub struct
    __rtic_internal_comport_consumerSharedResources < 'a >
    {
        #[allow(missing_docs)] pub uart2_rx : shared_resources ::
        uart2_rx_that_needs_to_be_locked < 'a > , #[doc(hidden)] pub
        __rtic_internal_marker : core :: marker :: PhantomData < & 'a () > ,
    } #[doc = r" Spawns the task directly"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_comport_consumer_spawn() -> :: core
    :: result :: Result < (), () >
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(comport_consumer, &
            __rtic_internal_comport_consumer_EXEC); if exec.try_allocate()
            {
                exec.spawn(comport_consumer(unsafe
                { comport_consumer :: Context :: new() })); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI1); Ok(())
            } else { Err(()) }
        }
    } #[doc = r" Gives waker to the task"] #[allow(non_snake_case)]
    #[doc(hidden)] pub fn __rtic_internal_comport_consumer_waker() -> :: core
    :: task :: Waker
    {
        unsafe
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(comport_consumer, &
            __rtic_internal_comport_consumer_EXEC);
            exec.waker(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(comport_consumer, &
                __rtic_internal_comport_consumer_EXEC); exec.set_pending();
                rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI1);
            })
        }
    } #[doc = r" Execution context"] #[allow(non_snake_case)]
    #[allow(non_camel_case_types)] pub struct
    __rtic_internal_comport_consumer_Context < 'a >
    {
        #[doc(hidden)] __rtic_internal_p : :: core :: marker :: PhantomData <
        & 'a () > , #[doc = r" Local Resources this task has access to"] pub
        local : comport_consumer :: LocalResources < 'a > ,
        #[doc = r" Shared Resources this task has access to"] pub shared :
        comport_consumer :: SharedResources < 'a > ,
    } impl < 'a > __rtic_internal_comport_consumer_Context < 'a >
    {
        #[inline(always)] #[allow(missing_docs)] pub unsafe fn new() -> Self
        {
            __rtic_internal_comport_consumer_Context
            {
                __rtic_internal_p : :: core :: marker :: PhantomData, local :
                comport_consumer :: LocalResources :: new(), shared :
                comport_consumer :: SharedResources :: new(),
            }
        }
    } #[allow(non_snake_case)] #[doc = "Software task"] pub mod
    comport_consumer
    {
        #[doc(inline)] pub use super ::
        __rtic_internal_comport_consumerLocalResources as LocalResources;
        #[doc(inline)] pub use super ::
        __rtic_internal_comport_consumerSharedResources as SharedResources;
        #[doc(inline)] pub use super :: __rtic_internal_comport_consumer_spawn
        as spawn; #[doc(inline)] pub use super ::
        __rtic_internal_comport_consumer_waker as waker; #[doc(inline)] pub
        use super :: __rtic_internal_comport_consumer_Context as Context;
    } #[allow(non_snake_case)] async fn blink_led < 'a >
    (mut cx : blink_led :: Context < 'a >)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; const
        TOGGLE_INTERVAL_MS : u32 = 1_000; let mut blink_count = 0_u32; loop
        {
            Mono :: delay(TOGGLE_INTERVAL_MS.millis()).await; let enabled =
            cx.shared.blink_enabled.lock(| enabled | * enabled); if enabled
            {
                let _ = StatefulOutputPin :: toggle(cx.local.led3);
                blink_count = blink_count.wrapping_add(1); report_blink ::
                spawn(blink_count).expect("blink report task queue must have capacity");
            } else { let _ = OutputPin :: set_low(cx.local.led3); }
        }
    } #[allow(non_snake_case)] async fn report_blink < 'a >
    (cx : report_blink :: Context < 'a > , count : u32)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let _ =
        cx; defmt :: info! ("Blink {}", count);
    } #[allow(non_snake_case)] async fn comport_consumer < 'a >
    (mut cx : comport_consumer :: Context < 'a >)
    {
        use rtic :: Mutex as _; use rtic :: mutex :: prelude :: * ; let mut
        bytes = [0_u8; UART_RX_BUFFER_SIZE]; loop
        {
            match cx.shared.uart2_rx.lock(| rx | rx.read_chunk(& mut bytes))
            {
                UartRxReadOutcome :: NoChunk => {} UartRxReadOutcome ::
                RecycleError =>
                { defmt :: warn! ("serial RX buffer recycle error"); }
                UartRxReadOutcome :: Chunk(len) =>
                {
                    cx.local.comport_decoder.consume(& bytes [.. len], | event |
                    match event
                    {
                        LineConsumerEvent :: Line(line) =>
                        {
                            match core :: str :: from_utf8(line.as_slice())
                            {
                                Ok(line) => defmt :: info! ("COMPORT: {}", line), Err(_) =>
                                defmt :: info! ("COMPORT bytes: {=[u8]}", line.as_slice()),
                            }
                        } LineConsumerEvent :: Overflow =>
                        {
                            defmt :: warn!
                            ("COMPORT line exceeded 64 bytes; discarded");
                        }
                    });
                }
            } Mono :: delay(1.millis()).await;
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic0"] static
    __rtic_internal_shared_resource_blink_enabled : rtic :: RacyCell < core ::
    mem :: MaybeUninit < bool >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: blink_enabled_that_needs_to_be_locked < 'a >
    {
        type T = bool; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut bool) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_blink_enabled.get_mut()
                as * mut _, CEILING, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS,
                f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic1"] static
    __rtic_internal_shared_resource_uart2 : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Uart2RxIrq >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: uart2_that_needs_to_be_locked < 'a >
    {
        type T = Uart2RxIrq; #[inline(always)] fn lock < RTIC_INTERNAL_R >
        (& mut self, f : impl FnOnce(& mut Uart2RxIrq) -> RTIC_INTERNAL_R) ->
        RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 3u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_uart2.get_mut() as * mut
                _, CEILING, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS, f,)
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic2"] static
    __rtic_internal_shared_resource_uart2_rx : rtic :: RacyCell < core :: mem
    :: MaybeUninit < UartRxParserSide >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit()); impl < 'a > rtic :: Mutex for
    shared_resources :: uart2_rx_that_needs_to_be_locked < 'a >
    {
        type T = UartRxParserSide; #[inline(always)] fn lock < RTIC_INTERNAL_R
        >
        (& mut self, f : impl FnOnce(& mut UartRxParserSide) ->
        RTIC_INTERNAL_R) -> RTIC_INTERNAL_R
        {
            #[doc = r" Priority ceiling"] const CEILING : u8 = 2u8; unsafe
            {
                rtic :: export ::
                lock(__rtic_internal_shared_resource_uart2_rx.get_mut() as *
                mut _, CEILING, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS, f,)
            }
        }
    } mod shared_resources
    {
        #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        blink_enabled_that_needs_to_be_locked < 'a >
        {
            __rtic_internal_p : :: core :: marker :: PhantomData <
            (& 'a (), * const u8) > ,
        } unsafe impl < 'a > Sync for blink_enabled_that_needs_to_be_locked <
        'a > {} impl < 'a > blink_enabled_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new() -> Self
            {
                blink_enabled_that_needs_to_be_locked
                { __rtic_internal_p : :: core :: marker :: PhantomData }
            }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        uart2_that_needs_to_be_locked < 'a >
        {
            __rtic_internal_p : :: core :: marker :: PhantomData <
            (& 'a (), * const u8) > ,
        } unsafe impl < 'a > Sync for uart2_that_needs_to_be_locked < 'a > {}
        impl < 'a > uart2_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new() -> Self
            {
                uart2_that_needs_to_be_locked
                { __rtic_internal_p : :: core :: marker :: PhantomData }
            }
        } #[doc(hidden)] #[allow(non_camel_case_types)] pub struct
        uart2_rx_that_needs_to_be_locked < 'a >
        {
            __rtic_internal_p : :: core :: marker :: PhantomData <
            (& 'a (), * const u8) > ,
        } unsafe impl < 'a > Sync for uart2_rx_that_needs_to_be_locked < 'a >
        {} impl < 'a > uart2_rx_that_needs_to_be_locked < 'a >
        {
            #[inline(always)] pub unsafe fn new() -> Self
            {
                uart2_rx_that_needs_to_be_locked
                { __rtic_internal_p : :: core :: marker :: PhantomData }
            }
        }
    } #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic3"] static
    __rtic_internal_local_resource_led3 : rtic :: RacyCell < core :: mem ::
    MaybeUninit < Pin < 'A', 5, Output < PushPull > > >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic4"] static
    __rtic_internal_local_resource_user_button : rtic :: RacyCell < core ::
    mem :: MaybeUninit < Pin < 'C', 13, Input > >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] #[link_section = ".uninit.rtic5"] static
    __rtic_internal_local_resource_comport_decoder : rtic :: RacyCell < core
    :: mem :: MaybeUninit < LineConsumer >> = rtic :: RacyCell ::
    new(core :: mem :: MaybeUninit :: uninit());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] static __rtic_internal_local_init_uart2_rx_buffers : rtic
    :: RacyCell < UartRxBufferBank > = rtic :: RacyCell ::
    new(ferrowasp_stm32f4 :: app_storage :: new_uart_rx_buffer_bank());
    #[allow(non_camel_case_types)] #[allow(non_upper_case_globals)]
    #[doc(hidden)] static __rtic_internal_local_init_uart2_free_queue : rtic
    :: RacyCell < UartRxFreeQueue > = rtic :: RacyCell ::
    new(UartRxFreeQueue :: new()); #[allow(non_camel_case_types)]
    #[allow(non_upper_case_globals)] #[doc(hidden)] static
    __rtic_internal_local_init_uart2_filled_queue : rtic :: RacyCell <
    UartRxFilledQueue > = rtic :: RacyCell :: new(UartRxFilledQueue :: new());
    #[allow(non_upper_case_globals)] static __rtic_internal_blink_led_EXEC :
    rtic :: export :: executor :: AsyncTaskExecutorPtr = rtic :: export ::
    executor :: AsyncTaskExecutorPtr :: new();
    #[allow(non_upper_case_globals)] static __rtic_internal_report_blink_EXEC
    : rtic :: export :: executor :: AsyncTaskExecutorPtr = rtic :: export ::
    executor :: AsyncTaskExecutorPtr :: new();
    #[allow(non_upper_case_globals)] static
    __rtic_internal_comport_consumer_EXEC : rtic :: export :: executor ::
    AsyncTaskExecutorPtr = rtic :: export :: executor :: AsyncTaskExecutorPtr
    :: new(); #[allow(non_snake_case)]
    #[doc = "Interrupt handler to dispatch async tasks at priority 1"]
    #[no_mangle] unsafe fn EXTI0()
    {
        #[doc = r" The priority of this interrupt handler"] const PRIORITY :
        u8 = 1u8; rtic :: export ::
        run(PRIORITY, ||
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(blink_led, & __rtic_internal_blink_led_EXEC);
            exec.poll(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(blink_led, & __rtic_internal_blink_led_EXEC);
                exec.set_pending(); rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0);
            }); let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_2_args(report_blink, &
            __rtic_internal_report_blink_EXEC);
            exec.poll(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_2_args(report_blink, &
                __rtic_internal_report_blink_EXEC); exec.set_pending(); rtic
                :: export :: pend(stm32f4xx_hal :: pac :: interrupt :: EXTI0);
            });
        });
    } #[allow(non_snake_case)]
    #[doc = "Interrupt handler to dispatch async tasks at priority 2"]
    #[no_mangle] unsafe fn EXTI1()
    {
        #[doc = r" The priority of this interrupt handler"] const PRIORITY :
        u8 = 2u8; rtic :: export ::
        run(PRIORITY, ||
        {
            let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
            from_ptr_1_args(comport_consumer, &
            __rtic_internal_comport_consumer_EXEC);
            exec.poll(||
            {
                let exec = rtic :: export :: executor :: AsyncTaskExecutor ::
                from_ptr_1_args(comport_consumer, &
                __rtic_internal_comport_consumer_EXEC); exec.set_pending();
                rtic :: export ::
                pend(stm32f4xx_hal :: pac :: interrupt :: EXTI1);
            });
        });
    } #[doc(hidden)] #[no_mangle] unsafe extern "C" fn main() -> !
    {
        rtic :: export :: assert_send :: < bool > (); rtic :: export ::
        assert_send :: < Uart2RxIrq > (); rtic :: export :: assert_send :: <
        UartRxParserSide > (); rtic :: export :: assert_send :: < Pin < 'A',
        5, Output < PushPull > > > (); rtic :: export :: assert_send :: < Pin
        < 'C', 13, Input > > (); rtic :: export :: assert_send :: <
        LineConsumer > (); rtic :: export :: assert_send :: < u32 > (); rtic
        :: export :: interrupt :: disable(); let mut core : rtic :: export ::
        Peripherals = rtic :: export :: Peripherals :: steal().into(); let _ =
        you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml ::
        interrupt :: EXTI0; let _ =
        you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml ::
        interrupt :: EXTI1; const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 1u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'EXTI0' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI0, rtic :: export ::
        cortex_logical2hw(1u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI0); const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 2u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'EXTI1' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI1, rtic :: export ::
        cortex_logical2hw(2u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI1); const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 2u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'EXTI15_10' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI15_10, rtic :: export ::
        cortex_logical2hw(2u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: EXTI15_10); const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 3u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'DMA1_STREAM5' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: DMA1_STREAM5, rtic :: export ::
        cortex_logical2hw(3u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: DMA1_STREAM5); const _ : () = if
        (1 << stm32f4xx_hal :: pac :: NVIC_PRIO_BITS) < 3u8 as usize
        {
            :: core :: panic!
            ("Maximum priority used by interrupt vector 'USART2' is more than supported by hardware");
        };
        core.NVIC.set_priority(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: USART2, rtic :: export ::
        cortex_logical2hw(3u8, stm32f4xx_hal :: pac :: NVIC_PRIO_BITS),); rtic
        :: export :: NVIC ::
        unmask(you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml
        :: interrupt :: USART2); #[inline(never)] fn __rtic_init_resources < F
        > (f : F) where F : FnOnce() { f(); } let mut executors_size = 0; let
        executor = :: core :: mem :: ManuallyDrop ::
        new(rtic :: export :: executor :: AsyncTaskExecutor ::
        new_1_args(blink_led));
        {
            executors_size += :: core :: mem :: size_of_val(& executor);
            __rtic_internal_blink_led_EXEC.set_in_main(& executor);
        } let executor = :: core :: mem :: ManuallyDrop ::
        new(rtic :: export :: executor :: AsyncTaskExecutor ::
        new_2_args(report_blink));
        {
            executors_size += :: core :: mem :: size_of_val(& executor);
            __rtic_internal_report_blink_EXEC.set_in_main(& executor);
        } let executor = :: core :: mem :: ManuallyDrop ::
        new(rtic :: export :: executor :: AsyncTaskExecutor ::
        new_1_args(comport_consumer));
        {
            executors_size += :: core :: mem :: size_of_val(& executor);
            __rtic_internal_comport_consumer_EXEC.set_in_main(& executor);
        } extern "C"
        { pub static _stack_start : u32; pub static __ebss : u32; } let
        stack_start = & _stack_start as * const _ as u32; let ebss = & __ebss
        as * const _ as u32; if stack_start > ebss
        {
            if rtic :: export :: msp :: read() <= ebss
            {
                :: core :: panic!
                ("Stack overflow after allocating executors");
            }
        }
        __rtic_init_resources(||
        {
            let (shared_resources, local_resources) =
            init(init :: Context :: new(core.into(), executors_size));
            __rtic_internal_shared_resource_blink_enabled.get_mut().write(core
            :: mem :: MaybeUninit :: new(shared_resources.blink_enabled));
            __rtic_internal_shared_resource_uart2.get_mut().write(core :: mem
            :: MaybeUninit :: new(shared_resources.uart2));
            __rtic_internal_shared_resource_uart2_rx.get_mut().write(core ::
            mem :: MaybeUninit :: new(shared_resources.uart2_rx));
            __rtic_internal_local_resource_led3.get_mut().write(core :: mem ::
            MaybeUninit :: new(local_resources.led3));
            __rtic_internal_local_resource_user_button.get_mut().write(core ::
            mem :: MaybeUninit :: new(local_resources.user_button));
            __rtic_internal_local_resource_comport_decoder.get_mut().write(core
            :: mem :: MaybeUninit :: new(local_resources.comport_decoder));
            rtic :: export :: interrupt :: enable();
        }); loop {}
    }
}