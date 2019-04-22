import subprocess
import sys


def call(command):
    print(command)
    r = subprocess.call(command, shell=True)
    if r != 0:
        sys.exit(r)


def clippy():
    deny = ['clippy::cast_lossless']
    call(f'cargo clippy -- -D {"".join(deny)}')


def make():
    call('cargo build')


path_rom_only = r"/tmp/gb/3D Wireframe Demo (PD) [C].gbc"
path_mbc1 = r"/tmp/gb/175 Sprite Parallax Starfield Demo (PD) [C].gb"
path_mbc1_ram = r"/tmp/gb/AGO Realtime Demo (LCP2000) (PD) [C].gbc"
path_mbc1_ram_battery = r"/tmp/gb/Boxes (PD).gb"
path_mbc2_battery = r"/tmp/gb/Fastest Lap (JU) [b1].gb"
path_mbc3_ram_battery = r"/tmp/gb/pokemon_blud.gb"
path_mbc3_timer_ram_battery = r"/tmp/gb/Pokemon - Crystal Version (US).gbc"
path_mbc5_ram_battery = r"/tmp/gb/Alice in Wonderland (U) [C][!].gbc"
path_cpu_instrs = r"/tmp/gb/cpu_instrs.gb"
path_cpu_instr_timing = r"/tmp/gb/instr_timing.gb"


def test():
    make()
    for p in [
        path_rom_only,
        path_mbc1,
        path_mbc1_ram,
        path_mbc1_ram_battery,
        path_mbc2_battery,
        path_mbc3_ram_battery,
        path_mbc3_timer_ram_battery,
        path_mbc5_ram_battery
    ]:
        call(f'target\\debug\\gameboy.exe "{p}"')


def main():
    make()
    call(f'target\\debug\\gameboy.exe -a "{path_mbc1_ram_battery}"')


if __name__ == '__main__':
    main()
