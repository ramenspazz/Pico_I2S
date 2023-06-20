# UAC2 class audio card using the Pimoroni Pico Audio Pack on the RP2040 board
This is an implimentation in rust of a 3 pin UAC2 class I2C audio device on the [Pimoroni Pico Audio Pack](https://shop.pimoroni.com/products/pico-audio-pack) using the Texas Instrument PCM5100A Digital Analogue Converter. This project uses the [rp-hal](https://github.com/rp-rs/rp-hal) library, and as their project is still not at a stable version, I am only including the direct source code for the audio player.

I personally recommend inserting this file into the examples folder of the rp-hal rust project folder and building with `cargo run --release --examples pio_audio` after connecting your rp2040 in upload mode due to potential changes in the rp-hal library until they reach a stable release.

This is currently not outputting the sample sine wave I generate in the `generate_sine_wave` function and I can not figure out why. Any help from interested parties is wanted!

# Roadmap
1) Get test data to output
2) configure the usb pll speed for 192khz or higher
3) expose the rp2040 as a UAC2 audio device to the host system
4) integrate with the [pimoroni pico display pack](https://shop.pimoroni.com/products/pico-display-pack?variant=32368664215635)
5) ??? more to come ???  
