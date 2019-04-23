# Gameboy

Full featured Cross-platform GameBoy emulator. **Forever boys!**.

```s
$ cargo run -- "roms/Boxes (PD).gb"
```

![sample.gif](./docs/sample.gif)

Gameboy is developed by Rust, and fully tested on Windows, Ubuntu and Mac.

# Control

```
                _n_________________
                |_|_______________|_|
                |  ,-------------.  |
                | |  .---------.  | |
                | |  |         |  | |
                | |  |         |  | |
                | |  |         |  | |
                | |  |         |  | |
                | |  `---------'  | |
                | `---------------' |
                |   _ GAME BOY      |
   Up           | _| |_         ,-. | ----> Z
Left/Right <--- ||_ O _|   ,-. "._,"|
  Down          |  |_|    "._,"   A | ----> X
                |    _  _    B      |
                |   // //           |
                |  // //    \\\\\\  | ----> Enter/BackSpace
                |  `  `      \\\\\\ ,
                |________...______,"
```
