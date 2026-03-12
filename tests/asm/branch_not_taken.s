.psx
.open "branch_not_taken.bin", 0
    ADDIU r1, r0, 1
    ADDIU r2, r0, 2
    BEQ   r1, r2, taken
    ADDIU r3, r0, 11
    ADDIU r4, r0, 22
taken:
    ADDIU r5, r0, 33
    NOP
.close

; Expected:
; r3 = 11
; r4 = 22
; r5 = 33
