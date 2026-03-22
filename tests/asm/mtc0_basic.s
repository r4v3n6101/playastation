.psx
.open "mtc0_basic.bin", 0
    LUI   r1, 0x1234
    ORI   r1, r1, 0x5678
    .word 0x40817000 ; MTC0 r1, c0r14 (EPC)
    NOP
.close

; Expected:
; Cop0.EPC = 0x12345678
