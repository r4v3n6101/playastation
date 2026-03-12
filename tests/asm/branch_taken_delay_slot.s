.psx
.open "branch_taken_delay_slot.bin", 0
    ADDIU r1, r0, 5
    ADDIU r2, r0, 5
    BEQ   r1, r2, taken
    ADDIU r3, r0, 11
    ADDIU r4, r0, 22
taken:
    ADDIU r5, r0, 33
    NOP
.close

; Expected:
; r3 = 11      ; delay slot executed
; r4 = 0       ; flushed/skipped
; r5 = 33
