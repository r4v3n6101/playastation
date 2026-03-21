.psx
.open "div_by_zero.bin", 0
    ADDIU r1, r0, 10
    ADDIU r2, r0, 0
    DIV   r1, r2
    MFLO  r3
    MFHI  r4
    NOP
.close

; Expected:
; NO exception
; HI/LO follow your chosen deterministic policy
