.psx
.open "reserved_instruction_exception.bin", 0
    .word 0xFFFFFFFF
    NOP
.close

; Expected:
; reserved instruction exception
; EPC = address of invalid instruction
; Cause.ExcCode = 10
