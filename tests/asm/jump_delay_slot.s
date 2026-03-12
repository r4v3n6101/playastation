.psx
.open "jump_delay_slot.bin", 0
    J     target
    ADDIU r1, r0, 77
    ADDIU r2, r0, 88
target:
    ADDIU r3, r0, 99
    NOP
.close

; Expected:
; r1 = 77      ; delay slot executed
; r2 = 0       ; skipped
; r3 = 99
