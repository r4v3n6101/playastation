.psx
.open "shift_basic.bin", 0
    ADDIU r1, r0, 1
    SLL   r2, r1, 4
    SRL   r3, r2, 2
    SRA   r4, r2, 2
    ADDIU r5, r0, 3
    SLLV  r6, r1, r5
    NOP
.close

; Expected:
; r2 = 16
; r3 = 4
; r4 = 4
; r6 = 8
