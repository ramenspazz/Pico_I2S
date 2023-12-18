#![no_std]
#![no_main]
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::blocking::delay::DelayMs;
use hal::gpio::{FunctionPio0, Pin};
use hal::pac;
use hal::pio::PIOExt;
use hal::Sio;
use panic_halt as _;
use rp2040_hal as hal;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

// constants
const XTAL_FREQ_HZ: u32 = 12_000_000u32;
const BASE_CLOCK: f32 = 125E06;
const TABLE_SIZE: usize = 1920;
const AMPLITUDE: i32 = 0x6FFFFF;
const FREQUENCY: f32 = 300.0;
const SAMPLE_RATE: f32 = 192_000.0;
const PI: f32 = 3.141592653589732385;
const BITSHIFT_ONE_BYTE: u8 = 8;

/// macro to split a 32bit floating point number into a u16 whole number portion and a
/// u8 fractional prortion, returned as a tuple.
macro_rules! split_float {
    ($value:expr) => {{
        let whole = $value as u16;
        let frac = (($value - whole as f32) * 256.0) as u8; // TODO: I suspect this might need to be changed to 256.0
        (whole, frac)
    }};
}

/// # Purose
/// Represents the lrck sample frequency to use, represented as its own data type to prevent
/// comparisons to numbers where ever possible.
/// # Members
/// - Freq32khz:    32khz lrck signal
/// - Freq44_1khz:  44.1khz lrck signal
/// - Freq48khz:    48khz lrck signal
/// - Freq96khz:    96khz lrck signal
/// - Freq192khz:   192khz lrck signal
/// - Freq384khz:   384khz lrck signal
enum SampleFrequency {
    #[allow(dead_code)] Freq32khz,
    #[allow(dead_code)] Freq44_1khz,
    #[allow(dead_code)] Freq48khz,
    #[allow(dead_code)] Freq96khz,
    #[allow(dead_code)] Freq192khz,
    #[allow(dead_code)] Freq384khz,
}

/// # Purpose
/// Casts at the byte level an i32 into an equivalent byte level
/// representation of the i32 but now stored into a u32 and padded to fit a 32bit size.
fn cast_to_u32_as_i32(num: i32, is_24bit: bool) -> u32 {
    // we need to allow overflow in the case that the MSB is the only active bit
    // in the number, as the data format expected by the PCM510xA audio stereo DAC
    #[allow(overflowing_literals)]
    let mut temp = 0x0_u32;
    let bytes_ptr: *const u8 = &num as *const i32 as *const u8;

    // Process all bytes but the last.
    let byte_count = if is_24bit { 3 } else { 4 };
    for i in 0..byte_count {
        let cur_byte: u8;
        unsafe {
            cur_byte = *bytes_ptr.offset(i as isize);
        }
        
        temp |= (cur_byte as u32) << (BITSHIFT_ONE_BYTE * i);
    }

    // Process the last byte if it's 24-bit data
    if is_24bit {
        let cur_byte: u8;
        unsafe {
            cur_byte = *bytes_ptr.offset(3);
        }
        let msb_removed_byte: u8 = cur_byte & 0x7F;
        temp |= (msb_removed_byte as u32) << 24;

        // Add back in the MSB `num` to `temp`
        if (cur_byte & 0x80) == 0x80 {
            // The MSB was a 1, add it back to the final number
            temp |= 0x8000_0000;
        }
    }

    temp
}


/// # Purpose
/// A function to bitreverse a number for sending little endian to a big endian style machine
fn bit_reverse(mut num: u32) -> u32 {
    let mut rev_num = 0;
    let mut bits = 31;

    while num != 0 {
        rev_num |= num & 1;
        rev_num <<= 1;
        num >>= 1;
        bits -= 1;
    }

    rev_num <<= bits;
    rev_num
}

/// # Purpose
/// Generates an array of u32 samples that represent an i32 value at the byte level
/// 
/// This is required due to limitations of the hal for passing data to the tx fifo.
/// As the data is converted to analoge from the bit representation of this data, there
/// is no problem with the unsafe nature of these operations and their resulting use for
/// this specific use case but should not in general be done.
fn generate_sine_wave(samples: &mut [u32]) {
    let omega = 2.0 * PI * FREQUENCY / SAMPLE_RATE;
    for i in 0..TABLE_SIZE {
        let angle = omega * i as f32;
        let sample = (AMPLITUDE as f32 * {
            let mut out_temp = 0.;
            let mut angle_temp = 0.;
            out_temp += angle;
            angle_temp = angle_temp * angle * angle;
            out_temp += angle_temp / 6.;
            out_temp += angle_temp * angle * angle / 120.;
            out_temp
        }) as i32;
        // samples[i] = cast_to_u32_as_i32(sample, true);
        samples[i] = bit_reverse(cast_to_u32_as_i32(sample, true));
    }
}

// Entry point to our bare-metal application.
#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // configure pins for Pio
    let mut led_pin = pins.gpio25.into_push_pull_output();
    let i2s_data: Pin<_, FunctionPio0, _> = pins.gpio9.into_function();
    let i2s_bck: Pin<_, FunctionPio0, _> = pins.gpio10.into_function();
    let i2s_lrck: Pin<_, FunctionPio0, _> = pins.gpio11.into_function();

    // PIN id for use inside of PIO
    let pin9_i2s_data = i2s_data.id().num;
    let pin10_i2s_bck: u8 = i2s_bck.id().num;
    let pin11_i2s_lrck: u8 = i2s_lrck.id().num;
    let _pin25_led: u8 = 0x19;

    // PIO program to output the data and bck signal together.
    // This code largely comes from the RP2040 datasheet on section 3.5.1 on page 330.
    // output rate: 1 bit / 2 clock cycles => 0.5bits/cycle
    // We need a bck of 64 times the sampling frequency, so
    let program_0 = pio_proc::pio_asm!(
        "
        // use sideset to reduce the total memory footprint and maximum frequency possible
        .side_set 1
        loop:
            // output data from the osr to GIPO pin 9 and side set pin 10
            // after 32 operations of this, the osr will be refilled
            pull ifempty noblock    side 0
            nop                     side 0
            out pins, 1             side 1
            jmp loop                side 1
        "
    );
    
    // PIO program to output the lrck signal.
    // Due to the need for a 192khz signal, that is an effective 192kbits/second
    // needed data rate, so we need to set the clock to match.
    // The clock divider: "The clock is based on the sys_clk and will execute an instruction every int + (frac/256) ticks."
    // From this, the tick rate is 0.5bits/tick * 125(mbit/s) / (int + frac/256)(bit/tick) = 192kbit/s
    // => 0.5*125E06/(int + frac/256) * bit/s = 192kbit/s giving int+frac/256 must be aprox 325.521.
    let program_1 = pio_proc::pio_asm!(
        "
        .side_set 1
        loop:
            nop         side 1
            jmp loop    side 0
        "
    );
    
    // Initialize and start PIO
    let (mut pio, sm0, sm1, _, _) = pac.PIO0.split(&mut pac.RESETS);
    let target_lrck_freq = SampleFrequency::Freq192khz; // TODO: hardcoded for now, selection comes later
    
    // Find the appropriate BCK range for the desired LRCK frequency.
    // All frequencies are listed in Hertz below, abreviation Hz, units of (1/second)
    // All frequencies are pulled from Table 11. BCK Rates (MHz) by LRCK Sample Rate for PCM510xA PLL Operation
    // From the "PCM510xA 2.1 VRMS, 112/106/100 dB Audio Stereo DAC with PLL and 32-bit, 384 kHz PCM Interface" data sheet
    // We are going to use a BCK frequency at 64 times the lrck signal. The PCM5100A will accept 32 or 64 times the sampling rate.
    let (lrck_freq, bck_freq): (f32, f32) = {
        match target_lrck_freq {
            SampleFrequency::Freq32khz => (32_000f32, 1.024E06_f32),
            SampleFrequency::Freq44_1khz => (44_100f32, 1.4112E06_f32),
            SampleFrequency::Freq48khz => (48_000f32, 1.536E06_f32),
            SampleFrequency::Freq96khz => (96_000f32, 3.072E06_f32),
            SampleFrequency::Freq192khz => (192_000f32, 6.144E06_f32),
            SampleFrequency::Freq384khz => (384_000f32, 12.288E06_f32),
        }
    };
    // let freq_offset = 1.04; // This saves the tolerance (4%)

    // clock divisor: 1/div (instructions/tick)
    // effective clock rate of PIO: 125M ticks / second * (1/div) instructions / tick => CLOCK_EFF := 125E06/div (1/seconds)
    // effective bit rate: CLOCK_EFF * 0.5 (transitions/tick) => 
    // 
    let LRCK_PIO_CYCLES_PER = 2.0f32;
    let CK_PIO_CYCLES_PER = 4.0f32;
    let lrck_div = (BASE_CLOCK / LRCK_PIO_CYCLES_PER) / lrck_freq;
    let bck_data_div = (BASE_CLOCK / CK_PIO_CYCLES_PER) / bck_freq;
    
    // the clock divisor requires a whole and fractional divisor, so we calculate them here
    let (bck_whole, bck_frac) = split_float!(bck_data_div);
    let (lrck_whole, lrck_frac) = split_float!(lrck_div);

    // TODO: Calculate USB PLL settings for a UAC2 audio device

    // Set up the state machines by installing our PIO programs into the state machines and get a handle to the tx fifo on sm0
    // for transitting data to the pio from the usb line.
    let installed = pio.install(&program_0.program).unwrap();
    let (mut sm0, _, mut tx0) = rp2040_hal::pio::PIOBuilder::from_program(installed)
        .out_pins(pin9_i2s_data, 1)
        .side_set_pin_base(pin10_i2s_bck)
        .clock_divisor_fixed_point(bck_whole, bck_frac)
        .pull_threshold(0)
        .build(sm0);
    sm0.set_pindirs([
        (pin9_i2s_data, hal::pio::PinDir::Output),
        (pin10_i2s_bck, hal::pio::PinDir::Output)]);

    let installed = pio.install(&program_1.program).unwrap();
    let (mut sm1, _, _) = rp2040_hal::pio::PIOBuilder::from_program(installed)
        .side_set_pin_base(pin11_i2s_lrck)
        .clock_divisor_fixed_point(lrck_whole, lrck_frac)
        .build(sm1);
    sm1.set_pindirs([
        (pin11_i2s_lrck, hal::pio::PinDir::Output)]);


    let mut samples = [0; TABLE_SIZE];
    generate_sine_wave(&mut samples);
    led_pin.set_high().unwrap();

    // Set up the watchdog driver - needed by the clock setup code
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    // Configure the clocks
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut timer = rp2040_hal::Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

    // Start both SMs at the same time
    let _group = sm0.with(sm1).sync().start();
    timer.delay_ms(500);

    // Write data to the TX FIFO    
    #[allow(clippy::empty_loop)]
    loop {
        for sample in samples.iter() {
            while tx0.is_full() {}
            tx0.write(*sample);
        }        
    }
}
