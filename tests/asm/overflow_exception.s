.psx
.open "overflow_exception.bin", 0
    LUI   r1, 0x7FFF
    ORI   r1, r1, 0xFFFF
    ADDIU r2, r0, 1
    ADD   r3, r1, r2
    NOP
.close

; Expected:
; overflow exception
; r3 must NOT be written
; EPC = address of ADD
; Cause.ExcCode = 12
