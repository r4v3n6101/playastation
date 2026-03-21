.psx
.open "break_exception.bin", 0
    BREAK
    NOP
.close

; Expected:
; exception taken
; EPC = address of BREAK
; Cause.ExcCode = 9
