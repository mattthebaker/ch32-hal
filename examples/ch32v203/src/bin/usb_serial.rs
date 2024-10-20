#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![feature(naked_functions)]

use ch32_hal::usbd::{Driver, Instance};
use ch32_hal::{self as hal, bind_interrupts, embassy, peripherals, usbd};
use embassy_executor::Spawner;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::Builder;
use embedded_hal_async::delay::DelayNs;
use hal::gpio::{Level, Output};
use hal::println;

bind_interrupts!(struct Irqs {
    USB_LP_CAN1_RX0 => usbd::InterruptHandler<peripherals::USBD>;
});

#[embassy_executor::main(entry = "qingke_rt::entry")]
async fn main(spawner: Spawner) -> ! {
    hal::debug::SDIPrint::enable();
    let mut config = hal::Config::default();
    config.rcc = hal::rcc::Config::SYSCLK_FREQ_144MHZ_HSE;
    let p = hal::init(config);
    hal::embassy::init();

    let driver = usbd::Driver::new(p.USBD, Irqs, p.PA12, p.PA11);

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB");
    config.serial_number = Some("12345678");

    // Required for windows compatibility.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    // config.device_class = 0xEF;
    // config.device_sub_class = 0x02;
    // config.device_protocol = 0x01;
    // config.composite_with_iads = true;
    config.max_packet_size_0 = 8;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    // let mut device_descriptor = [0; 32];
    let mut config_descriptor = [0; 128];
    let mut bos_descriptor = [0; 128];
    let mut control_buf = [0; 16];

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        // &mut device_descriptor,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // GPIO
    let mut led = Output::new(p.PB2, Level::Low, Default::default());
    led.set_high();

    // Create classes on the builder.
    let mut class = CdcAcmClass::new(&mut builder, &mut state, 64);

    //// Build the builder.
    let mut usb = builder.build();

    led.set_low();

    // // Run the USB device.
    let usb_fut = usb.run();

    // Do stuff with the class!
    let echo_fut = async {
        loop {
            class.wait_connection().await;
            println!("Connected");
            let _ = echo(&mut class).await;
            println!("Disconnected");
        }
    };

    let led_fut = async {
        loop {
            led.set_high();
            embassy_time::Delay.delay_ms(250).await;
            led.set_low();
            embassy_time::Delay.delay_ms(250).await;
        }
    };

    println!("Join");
    // Run everything concurrently.
    // If we had made everything `'static` above instead, we could do this using separate tasks instead.
    embassy_futures::join::join3(usb_fut, echo_fut, led_fut).await;
    loop {}
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d, T: Instance + 'd>(class: &mut CdcAcmClass<'d, Driver<'d, T>>) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        //println!("data: {:x}", data);
        class.write_packet(data).await?;
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = println!("\n\n\n{}", info);

    loop {}
}
