;==============================
; syscall_exception.s
;==============================
    SYSCALL
    NOP

; Expected:
; exception taken
; EPC = address of SYSCALL
; Cause.ExcCode = 8


;==============================
; break_exception.s
;==============================
    BREAK
    NOP

; Expected:
; exception taken
; EPC = address of BREAK
; Cause.ExcCode = 9


;==============================
; overflow_exception.s
;==============================
    LUI   r1, 0x7FFF
    ORI   r1, r1, 0xFFFF
    ADDIU r2, r0, 1
    ADD   r3, r1, r2
    NOP

; Expected:
; overflow exception
; r3 must NOT be written
; EPC = address of ADD
; Cause.ExcCode = 12


;==============================
; reserved_instruction_exception.s
;==============================
    .word 0xFFFFFFFF
    NOP

; Expected:
; reserved instruction exception
; EPC = address of invalid instruction
; Cause.ExcCode = 10


;==============================
; misaligned_lw_exception.s
;==============================
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1001
    LW    r2, 0(r1)
    NOP

; Expected:
; address error on load
; EPC = address of LW
; Cause.ExcCode = 4


;==============================
; misaligned_sw_exception.s
;==============================
    LUI   r1, 0x0000
    ORI   r1, r1, 0x1001
    ADDIU r2, r0, 123
    SW    r2, 0(r1)
    NOP

; Expected:
; address error on store
; EPC = address of SW
; Cause.ExcCode = 5


;==============================
; div_normal.s
;==============================
    ADDIU r1, r0, 10
    ADDIU r2, r0, 3
    DIV   r1, r2
    MFLO  r3
    MFHI  r4
    NOP

; Expected:
; r3 = 3
; r4 = 1


;==============================
; div_by_zero.s
;==============================
    ADDIU r1, r0, 10
    ADDIU r2, r0, 0
    DIV   r1, r2
    MFLO  r3
    MFHI  r4
    NOP

; Expected:
; NO exception
; HI/LO follow your chosen deterministic policy


;==============================
; delay_slot_exception_bd.s
;==============================
    BEQ   r0, r0, target
    SYSCALL
    NOP
target:
    NOP

; Expected:
; exception taken from delay slot
; Cause.BD = 1
; EPC = address of BEQ
; ExcCode = 8
