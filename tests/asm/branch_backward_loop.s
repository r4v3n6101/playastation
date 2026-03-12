.psx
.open "branch_backward_loop.bin", 0
    ADDIU r1, r0, 3
loop:
    ADDIU r2, r2, 1
    ADDIU r1, r1, -1
    BNE   r1, r0, loop
    NOP
.close

; Expected:
; r1 = 0
; r2 = 3
