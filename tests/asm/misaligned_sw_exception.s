.psx
.open "misaligned_sw_exception.bin", 0
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1001
    ADDIU r2, r0, 123
    SW    r2, 0(r1)
    NOP
.close

; Expected:
; address error on store
; EPC = address of SW
; Cause.ExcCode = 5
