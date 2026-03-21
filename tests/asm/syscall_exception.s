.psx
.open "syscall_exception.bin", 0
    SYSCALL
    NOP
.close

; Expected:
; exception taken
; EPC = address of SYSCALL
; Cause.ExcCode = 8
