# UAC2 class audio card using the Pimoroni Pico Audio Pack on the RP2040 board
This is an implimentation in rust of a 3 pin I2C audio device using the Texas Instrument PCM5100A Digital Analogue Converter. This project uses the rp-hal library, and as their project is still not at a stable version, I am only including the direct source code for the audio player.
I personally recomend inserting this file into the examples folder of the rust project and building with `cargo run --release --examples pio_audio` after connecting your rp2040 in upload mode. 
