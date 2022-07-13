    .p2align 4
    .global ___lumen_swap_stack
___lumen_swap_stack:
    .cfi_startproc
    .cfi_personality 155, _rust_eh_personality
    .cfi_lsda 255
    # At this point the following registers are bound:
    #
    #   rdi <- prev: *mut CalleeSavedRegisters
    #   rsi <- new: *const CalleeSavedRegisters
    #   rdx <- FIRST_SWAP (needs to be in a register because 64-bit constants can't be encoded in `cmp*` instructions directly, so need to use reg64, reg64 form.)
    #

    # Save the return address to a register
    lea  rax,  [rip + L_ret]

    # Save the parent base pointer for when control returns to this call frame.
    # CFA directives will inform the unwinder to expect rbp at the bottom of the
    # stack for this frame, so this should be the last value on the stack in the caller
    push rbp

    # We also save rbp and rsp to registers so that we can setup CFA directives if this
    # is the first swap for the target process
    mov  rcx,        rbp
    mov  r9,         rsp

    # Save the stack pointer, and callee-saved registers of `prev`
    mov  [rdi],      rsp
    mov  [rdi + 8],  r15
    mov  [rdi + 16], r14
    mov  [rdi + 24], r13
    mov  [rdi + 32], r12
    mov  [rdi + 40], rbx
    mov  [rdi + 48], rbp

    # Restore the stack pointer, and callee-saved registers of `new`
    mov  rsp,        [rsi]
    mov  r15,        [rsi + 8]
    mov  r14,        [rsi + 16]
    mov  r13,        [rsi + 24]
    mov  r12,        [rsi + 32]
    mov  rbx,        [rsi + 40]
    mov  rbp,        [rsi + 48]

    # The value of all the callee-saved registers has changed, so we
    # need to inform the unwinder of that fact before proceeding
    .cfi_restore rsp
    .cfi_restore r15
    .cfi_restore r14
    .cfi_restore r13
    .cfi_restore r12
    .cfi_restore rbx
    .cfi_restore rbp

    # If this is the first time swapping to this process,
    # we need to to perform some one-time initialization to
    # link the stack to the original parent stack (i.e. the scheduler),
    # which is important for the unwinder
    cmp  rdx,        r13
    jne  L_resume

    # Ensure we never perform initialization twice
    mov  r13,        0x0
    # Store the original base pointer at the top of the stack
    push rcx
    # Followed by the return address
    push rax
    # Finally we store a pointer to the bottom of the stack in the
    # parent call frame. The unwinder will expect to restore rbp
    # from this address
    push r9

    # These CFI directives inform the unwinder of where it can expect
    # to find the CFA relative to rbp. This matches how we've laid out the stack.
    #
    # - The current rbp is now 24 bytes (3 words) above rsp.
    # - 16 bytes _down_ from the current rbp is the value from r9 that
    # we pushed, containing the parent call frame's stack pointer.
    #
    # The first directive tells the unwinder that it can expect to find the
    # CFA (call frame address) 16 bytes above rbp. The second directive then
    # tells the unwinder that it can find the previous rbp 16 bytes _down_
    # from the current rbp. The result is that the unwinder will restore rbp
    # from that stack slot, and will then expect to find the previous CFA 16 bytes
    # above that address, allowing the unwinder to walk back into the parent frame
    .cfi_def_cfa rbp, 16
    .cfi_offset rbp, -16

    # Now that the frames are linked, we can call the entry point. For now, this
    # is __lumen_trap_exceptions, which expects to receive two arguments: the function
    # being wrapped by the exception handler, and the value of the closure environment,
    # _if_ it is a closure being called, otherwise the value of that argument is Term::NONE
    mov  rdi,        r12

    # We have already set up the stack precisely, so we don't use call here, instead
    # we go ahead and jump straight to the beginning of the entry function.
    # NOTE: This call never truly returns, as the exception handler calls __lumen_builtin_exit
    # with the return value of the 'real' entry function, or with an exception if one
    # is caught. However, swap_stack _does_ return for all other swaps, just not the first.
    jmp  [r15]

L_resume:
    # We land here only on a context switch, and since the last switch _away_ from
    # this process pushed rbp on to the stack, and we don't need that value, we
    # adjust the stack pointer accordingly.
    add rsp,         8

L_ret:
    # At this point we will return back to where execution left off:
    # For the 'root' (scheduler) process, this returns back into `swap_process`;
    # for all other processes, this returns to the code which was executing when
    # it yielded, the address of which is 8 bytes above the current stack pointer.
    # We pop and jmp rather than ret to avoid branch mispredictions.
    pop  rax
    jmp  [rax]

    .cfi_endproc
