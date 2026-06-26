# Elitra OS

**Статус:** хобби-ОС в стадии активной разработки. Работающее 64-битное ядро для x86-64 с базовым набором драйверов, файловыми системами и мультизадачностью. Ядро написано на C++ и Rust (krust), пользовательские утилиты — на Rust (rust-rt).

## Возможности

### Ядро
- Загрузка через GRUB (Multiboot2) или напрямую QEMU (PVH ELF Note)
- **64-bit long mode** (x86-64), 4-уровневая страничная память с huge pages (2MB)
- Физический аллокатор (PMM bitmap) + кучевой аллокатор (Heap)
- Прерывания (IDT, 8259 PIC) + Task State Segment (TSS)
- FPU/SSE контекст (FXSAVE/FXRSTOR)
- GDT для 64-битного режима (long mode)
- CPUID, системный таймер (PIT)

### Драйверы
- VGA текст (80×25)
- Framebuffer (VBE 1024×768×32, инициализируется через GRUB)
- **Терминал** с 4 виртуальными терминалами (F1–F4), скроллбэком (PgUp/PgDn), часами в статус-баре
- PS/2 клавиатура (US раскладка) + мышь
- CMOS RTC (часы реального времени)
- PIT таймер (100 Гц)
- ATA PIO + DMA (Bus Mastering), write-back кэш для FAT32
- Intel e1000 (сетевой адаптер — драйвер есть, ARP/UDP поверх)
- ACPI (RSDP/RSDT/FADT/DSDT) — выключение и перезагрузка
- Последовательный порт (NS16550, отладка)

### Файловые системы
- **FAT32** — чтение/запись (реализация на Rust в krust, вызываемая из C++)
- **ext2** — чтение/запись (блочный аллокатор, косвенная адресация, поддержка директорий)
- **VFS** — единая виртуальная файловая система
- Mount table, pipe, перенаправление вывода

### Мультизадачность
- Кооперативная + вытесняющая (PIT, 100 Гц)
- Сохранение/восстановление FPU/SSE контекста
- ELF-загрузчик (64-битные статические ELF)
- ~30 системных вызовов (int 0x80)
- User-mode процессы (кольцо 3)

### Пользовательское пространство
- Встроенная оболочка (kernel shell) с 20+ командами
- 24 ELF-программы на Rust (init, cat, ls, echo, cp, mv, pwd, tee, uname, ...)
- Поддержка пайпов (`|`) и перенаправления (`>`)
- Запуск ELF-программ через `exec`
- Init процесс (автозапуск при наличии `/mnt/bin/init.elf`)

## Архитектура

| Компонент | Язык | Строк кода | Доля |
|---|---|---|---|
| Ядро, драйверы, ФС, менеджер памяти | C++ | ~10 700 | 73% |
| Библиотека ядра (FAT32, PMM, VGA, serial) | Rust (krust) | ~980 | 7% |
| Пользовательские утилиты (24 .elf) | Rust (rust-rt) | ~2 000 | 14% |
| Загрузчик, прерывания, переключение контекста | asm (nasm) | ~600 | 4% |
| Сборочные скрипты, linker scripts | Makefile, ld, toml | ~500 | 2% |

```
                 .─────────.
                (  GRUB/PVH )
                 `────┬────'
                      │ boot.asm (32→64)
                      ▼
         ┌──────────────────────────┐
         │  C++ ядро (kernel_main)  │◄──── krust (Rust staticlib)
         │  драйверы, VFS, sched    │
         └────────┬─────────────────┘
                  │ int 0x80
                  ▼
         ┌──────────────────────────┐
         │  rust-rt userspace .elf  │
         │  init, ls, cat, echo ... │
         └──────────────────────────┘
```

## Сборка и запуск

### Быстрый запуск (PVH, TCG)
```
make -j$(nproc)
qemu-system-x86_64 -kernel elitra-kernel.bin -m 256M \
  -drive file=fat32.img,format=raw,index=0,media=disk \
  -machine accel=tcg -nographic -no-reboot -no-shutdown
```

### Запуск через GRUB (ISO)
```
make -j$(nproc)
rm -rf iso && mkdir -p iso/boot/grub
cp elitra-kernel.bin iso/boot/kernel.elf
cat > iso/boot/grub/grub.cfg << 'GRUB'
set timeout=0
set default=0
menuentry "Elitra-OS" {
    multiboot2 /boot/kernel.elf
}
GRUB
grub-mkrescue -o elitra.iso iso
qemu-system-x86_64 -cdrom elitra.iso -m 256M \
  -drive file=fat32.img,format=raw,index=0,media=disk \
  -nographic -no-reboot -no-shutdown
```

### Зависимости
- `g++`, `nasm`, `ld` (binutils)
- `rustc +nightly` (для krust и пользовательских программ)
- `cargo +nightly -Z build-std`
- `mtools` (mcopy, mmd — для сборки fat32.img)
- `grub-mkrescue` (для ISO-образа)
- `qemu-system-x86_64`

## Команды оболочки

| Команда | Описание |
|---|---|
| `?` / `help` | Справка |
| `clr` | Очистить экран |
| `echo <text>` | Вывести текст |
| `list [path]` | Содержимое директории |
| `dump <file>` | Вывести файл |
| `create <path>` | Создать файл |
| `del <path>` | Удалить файл |
| `md <path>` | Создать директорию |
| `put <path> <text>` | Записать текст в файл |
| `exec <file>` | Запустить ELF |
| `mem` | Информация о памяти |
| `cpu` | Информация о CPU |
| `upt` | Время работы |
| `ver` | Версия ядра |
| `rst` | Перезагрузка |
| `off` | Выключение |
| `mnt` | Смонтированные ФС |
| `unm <path>` | Отмонтировать ФС |
| `sync` | Сброс на диск |
| `ata` | Информация об ATA дисках |
| `fs` | Информация о VFS |
| `jobs` | Информация о задачах |
| `say <text>` | Печать текста |
| `newt` | Создать тестовые задачи |
| `mall` | Тест аллокатора |
| `pt` | Тест страничной памяти |

Управление терминалом: **F1–F4** — виртуальные терминалы, **PgUp/PgDn** — скроллбэк.

## Changelog (последние изменения)

- Исправлен баг загрузки 64-bit: inverted page table identity mapping (вызывал triple fault при `mov cr0, PG`)
- Добавлена ELF-нота PVH (`.note.Xen`) для прямой загрузки через `qemu -kernel`
- Добавлен Multiboot2 header для загрузки через GRUB
- Serial debug output на COM1 в boot.asm для диагностики
- Миграция rust-rt/krust на `x86_64-unknown-none` (встроенный target Rust) вместо кастомного `.json`
- Добавлены секции `.init_array`, `.fini_array`, `.got`, `.got.plt`, `.eh_frame` в linker.ld, исправлен PHDRS для `.note`
- Исправлен spinlock (volatile + `__atomic_test_and_set` + `pause`)
- Отключены прерывания e1000 (предотвращение interrupt storm без хендлера)
- Добавлена доставка сигналов в syscall и yield
- Исправлены утечки памяти в ext2 init

## Чего нет (планируется)

- User/kernel mode (кольца защиты) — **реализовано**
- Init процесс и автозапуск — **реализовано** (требуется init.elf на FAT32)
- Сетевой стек (TCP/IP)
- AHCI/NVMe/Sound
- USB
- Оконная система
- Динамический линковщик
- Многопроцессорность (SMP)
- IRQ-управляемый приём e1000 (прерывания e1000 отключены)
- Загрузка пользовательских ELF через syscall execve

## Лицензия

MIT
