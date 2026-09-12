/* pio_rp1.c with its chip-registration entry marked SHF_GNU_RETAIN.
 * piolib finds chips by scanning the "piochips" section between the
 * linker's __start_/__stop_ symbols; lld garbage-collects a section
 * only reachable that way (GNU ld keeps it), so the entry is marked
 * retained for both linkers. */
#include "piolib_priv.h"
#undef DECLARE_PIO_CHIP
#define DECLARE_PIO_CHIP(chip) \
    const PIO_CHIP_T *__ptr_##chip __attribute__((section("piochips"))) \
        __attribute__((used, retain)) = &chip
#include "pio_rp1.c"
