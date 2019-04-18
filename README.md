Full featured GameBoy emulator. Let's dance!

```s
$ cargo run -- "mods/Boxes (PD).gb"
```

![sample.gif](./docs/sample.gif)

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
   Left/Right <---- ||_ O _|   ,-. "._,"|
      Down          |  |_|    "._,"   A | ----> X
                    |    _  _    B      |
                    |   // //           |
                    |  // //    \\\\\\  | ----> Enter/BackSpace
                    |  `  `      \\\\\\ ,
                    |________...______,"
```

# Implemented

- [x] Item: GameBoy and GameBoy Color
- [x] CPU: The sharp LR35902
- [ ] GPU: Need time to learn more
- [ ] APU: Need time to learn more
- [x] Cartridge
    - ROM ONLY
    - MBC1
    - MBC1+RAM
    - MBC1+RAM+BATTERY
    - MBC2
    - MBC2+BATTERY
    - ROM+RAM
    - ROM+RAM+BATTERY
    - MBC3+TIMER+BATTERY
    - MBC3+TIMER+RAM+BATTERY
    - MBC3
    - MBC3+RAM
    - MBC3+RAM+BATTERY
    - MBC5
    - MBC5+RAM
    - MBC5+RAM+BATTERY
    - MBC5+RUMBLE
    - MBC5+RUMBLE+RAM
    - MBC5+RUMBLE+RAM+BATTERY
    - HuC1+RAM+BATTERY
- [x] Joypad
- [x] MotherBoard
- [x] Timer

# Reference
- [https://github.com/gbdev/awesome-gbdev](https://github.com/gbdev/awesome-gbdev)
- [http://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html](http://www.pastraiser.com/cpu/gameboy/gameboy_opcodes.html)
- [https://ladecadence.net/trastero/listado%20juegos%20gameboy.html](https://ladecadence.net/trastero/listado%20juegos%20gameboy.html)
- [http://romhustler.net/roms/gbc/number](http://romhustler.net/roms/gbc/number)
- [http://gbdev.gg8.se/wiki/articles/CPU_Comparision_with_Z80](http://gbdev.gg8.se/wiki/articles/CPU_Comparision_with_Z80)
- [https://github.com/PoschR/Gameboy-Learning-Environment](https://github.com/PoschR/Gameboy-Learning-Environment)
- [https://mgba-emu.github.io/gbdoc/](https://mgba-emu.github.io/gbdoc/)
- [http://gbdev.gg8.se/wiki/articles/Main_Page](http://gbdev.gg8.se/wiki/articles/Main_Page)
- [https://gekkio.fi/files/gb-docs/gbctr.pdf](https://gekkio.fi/files/gb-docs/gbctr.pdf)
- [http://gameboy.mongenel.com/dmg/asmmemmap.html](http://gameboy.mongenel.com/dmg/asmmemmap.html)
- [http://bgb.bircd.org/pandocs.htm](http://bgb.bircd.org/pandocs.htm)

# Licences

WTFPL. Note: Some codes of screen and sounds copied from rboy(about ~1000 lines).
