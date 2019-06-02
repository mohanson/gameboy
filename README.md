# Gameboy

Full featured Cross-platform GameBoy emulator. **Forever boys!**.

```s
$ cargo run -- "./res/boxes.gb"
```

![sample.gif](./res/imgs/sample.gif)

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

# Tests

Thanks for [Blargg's Gameboy hardware test ROMs](https://github.com/retrio/gb-test-roms), I can simply verify my code. You can find the way how to run tests in `./Cargo.py`.


| Test Name    | Result                               |
|--------------|--------------------------------------|
| cpu_instrs   | ![img](./res/imgs/cpu_instrs.png)   |
| instr_timing | ![img](./res/imgs/instr_timing.png) |

# Reference

- [Gbdev](http://gbdev.gg8.se/wiki/articles/Main_Page)
- [Open Game Boy Documentation Project](https://mgba-emu.github.io/gbdoc/)
- [LR35902 Opcodes](https://rednex.github.io/rgbds/gbz80.7.html)
- [LR35902 Opcodes Table](http://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html)
- [GameBoy Memory Map](http://gameboy.mongenel.com/dmg/asmmemmap.html)
- [Game Boy Technical Data](http://bgb.bircd.org/pandocs.htm)
- [awesome-gbdev](https://github.com/gbdev/awesome-gbdev)
- [List of MBC roms](https://ladecadence.net/trastero/listado%20juegos%20gameboy.html)
- [Roms download](http://romhustler.net/roms/gbc/number)

# Licenses

WTFPL.
