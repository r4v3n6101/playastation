.psx
.open "mem_basic.bin", 0
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1000
    ADDIU r2, r0, 0x1234
    SW    r2, 0(r1)
    LW    r3, 0(r1)
    NOP
    NOP
.close

; Expected:
; mem[0x1000] = 0x00001234
; r3 = 0x00001234
