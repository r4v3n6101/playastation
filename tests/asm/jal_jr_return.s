.psx
.open "jal_jr_return.bin", 0
    JAL   func
    ADDIU r10, r0, 1

    ADDIU r11, r0, 2
    J     done
    NOP

func:
    ADDIU r12, r0, 3
    JR    r31
    ADDIU r13, r0, 4

done:
    NOP
.close

; Expected:
; r10 = 1
; r11 = 2
; r12 = 3
; r13 = 4
; r31 = 8
