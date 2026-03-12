.psx
.open "load_use_hazard.bin", 0
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1000
    ADDIU r2, r0, 55
    SW    r2, 0(r1)
    LW    r3, 0(r1)
    ADD   r4, r3, r3
    NOP
    NOP
.close

; Diagnostic:
; If you model raw 5-stage timing naively, this may use old r3.
; If you model architectural behavior/interlock correctly, r4 should become 110.
