.psx
.open "div_normal.bin", 0
    ADDIU r1, r0, 10
    ADDIU r2, r0, 3
    DIV   r1, r2
    MFLO  r3
    MFHI  r4
    NOP
.close

; Expected:
; r3 = 3
; r4 = 1
