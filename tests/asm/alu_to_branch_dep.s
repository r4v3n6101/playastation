.psx
.open "alu_to_branch_dep.bin", 0
    ADDIU r1, r0, 5
    ADDIU r2, r0, 5
    SUB   r3, r1, r2
    BEQ   r3, r0, equal
    ADDIU r4, r0, 1
    ADDIU r5, r0, 2
equal:
    ADDIU r6, r0, 3
    NOP
.close

; Expected on sane implementation:
; r4 = 1
; r5 = 0
; r6 = 3
