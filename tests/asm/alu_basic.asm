.psx
.open "alu_basic.bin", 0
    ADDIU r1, r0, 5
    ADDIU r2, r0, 7
    ADD   r3, r1, r2
    SUB   r4, r3, r1
    OR    r5, r1, r2
    AND   r6, r1, r2
    XOR   r7, r1, r2
    NOP
.close

; Expected:
; r1 = 5
; r2 = 7
; r3 = 12
; r4 = 7
; r5 = 7
; r6 = 5
; r7 = 2
