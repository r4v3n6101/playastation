.psx
.open "delay_slot_exception_bd.bin", 0
    BEQ   r0, r0, target
    ; armips rejects SYSCALL here, so emit the raw opcode in the delay slot.
    .word 0x0000000C
    NOP
target:
    NOP
.close

; Expected:
; exception taken from delay slot
; Cause.BD = 1
; EPC = address of BEQ
; ExcCode = 8
