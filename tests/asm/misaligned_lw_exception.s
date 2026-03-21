.psx
.open "misaligned_lw_exception.bin", 0
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1001
    LW    r2, 0(r1)
    NOP
.close

; Expected:
; address error on load
; EPC = address of LW
; Cause.ExcCode = 4
