.psx
.open "mem_offset_signed.bin", 0
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1008
    ADDIU r2, r0, 99
    SW    r2, -8(r1)
    LW    r3, -8(r1)
    NOP
    NOP
.close

; Expected:
; mem[0x1000] = 99
; r3 = 99
