.psx
.open "mfc0_basic.bin", 0
    LUI   r1, 0x1234
    ORI   r1, r1, 0x5678
    .word 0x40817000 ; MTC0 r1, c0r14 (EPC)
    .word 0x40027000 ; MFC0 r2, c0r14 (EPC)
    NOP              ; MFC0 load delay
    ADDU  r3, r2, r0
.close

; Expected:
; R3 = 0x12345678
