# Zen4 Register Quick Reference

This file is a quick, non-authoritative summary intended for designing hot loops, microkernels, and register plans for Zen4.

Sources and licensing notes: see `arch/zen4/README.md`.

## 1. Architectural and scalar registers

### 1.1 GPR set and addressing rules
- Zen4 executes AMD64 instructions through macro/micro-ops with fixed fields and a deliberately large internal register set to raise performance and keep the core extensible.
- Any fastpath-single instruction with a memory operand becomes a fastpath double when its addressing form uses two register sources (base+index or base+index+displacement), so compilers should avoid those forms whenever possible.
- Each SMT thread exposes six fully symmetric core performance counters, providing an independent bank of counter registers per thread.

### 1.2 Zeroing idioms for GPRs
- The core recognizes "zeroing idioms" that clear a register without loading an immediate and therefore break dependency chains.
- `XOR reg, reg` clears the destination register and all flags in zero cycles.
- `SUB reg, reg` does the same, also with zero latency.
- `CMP reg, reg` leaves only ZF set and clears the other flags without delay.
- `SBB reg, reg` copies the zero-extended Carry flag into the register without depending on the old value, taking one cycle.

### 1.3 Zero-cycle register moves
- The following register-to-register moves are zero-latency: `MOV r32/r64, r32/r64`, `MOVSXD r32, r32`, `XCHG EAX/RAX, r32/r64`, `XCHG r32/r64, r32/r64`, `(V)MOVAP(D/S)` between SIMD registers, and `(V)MOVDQ(U/A)` between SIMD registers.

### 1.4 Stack pointer tracking (rSP)
- The rename unit can track implicit stack-pointer updates so later instructions no longer depend on older rSP-modifying uops.
- Tracking covers `PUSH` (except `PUSH rSP`), `POP` (except `POP rSP`), `CALL` near rel/abs, and `RET` near/near imm.
- Loads/stores that use rSP as base or index, `MOV reg, rSP`, and `LEA` with `[rSP + disp]`, `[rSP]`, or `[rSP + index * scale + disp]` consume the tracked value without re-introducing dependencies.
- Any rSP update or use outside those lists causes an extra uop and resets tracking until a supported update occurs again.

### 1.5 RIP access and branch fusion
- Instead of `CALL 0`, 64-bit code can read the instruction pointer into a GPR via RIP-relative `LEA`, e.g., `LEA RAX, [RIP+0]`.
- The decode unit can fuse a conditional branch with an immediately preceding flag-writing instruction (CMP/TEST/SUB/ADD/INC/DEC/OR/AND/XOR), eliminating one macro-op.
- Fusion is disallowed for JCXZ, for flag writers containing both an immediate and a displacement, and for RIP-relative flag writers; the combined pair must be 15 bytes or less.
- When CMP mixes a register with a memory operand, place memory second (opcodes 0x3A/0x3B) to retain peak throughput.

### 1.6 DIV/IDIV fusion
- Zen4 fuses common sequences like `XOR rDX, rDX` + `DIV reg` and `CDQ/CQO` + `IDIV reg` if the register operand is not rDX and the operand sizes match. Memory operands do not participate.

### 1.7 Other GPR considerations
- Integer multiply instructions that write two destination registers (hi/lo products in rDX:rAX) incur extra latency for the second result.
- A flat memory model (64-bit mode or 32-bit with `CS.Base = 0` and `CS.Limit = FFFFFFFFh`) is required to leverage the Op Cache and its register-dependent optimizations.

### 1.8 Load latency adjustments by destination type
- Latency tables are typically for register-to-register forms; loads from memory must add cache-hit penalties depending on the destination register file.
- For GPR destinations add 4 cycles for an L1D hit, 14 for L2, and ~50 for L3.
- For FP/SIMD destinations add 7 cycles for L1D, 17 for L2, and ~53 for L3.
- Complex addressing (scaled index), non-zero segment bases, misaligned operands, and 512-bit loads each add another cycle; AVX-512 merge-masked loads can further raise latency or lower throughput.

## 2. Vector and floating-point registers

### 2.1 FPU organization
- The floating-point unit operates as a coprocessor for x87, MMX, XMM, YMM, ZMM, and FP control/status registers, with its own scheduler, register file, and renamer separate from the integer side.
- The FP scheduler can dispatch six macro-ops per cycle, issues one micro-op per pipe, tracks 2x32 macro-ops, and can overflow into a Non-Scheduling Queue for address-calculation acceleration.
- The unit accepts up to two 256-bit loads per cycle and includes dedicated buses to move data between FP registers and the GPR domain; two store-data pipelines serve FP stores and FP->GPR transfers.
- AVX-512 is supported with 512-bit storage in the FP register file; on Zen4 512-bit ops can issue over two cycles because datapaths are 256-bit wide.

### 2.2 Register width usage
- Peak efficiency typically comes from consistent full-width use (256-bit YMM or 512-bit ZMM) where appropriate, to reduce macro-op overhead.
- Load/store full registers in one instruction whenever possible (e.g. `vmovapd` rather than partial loads); if multiple loads are unavoidable, place them back to back.
- Heavy FP->GPR traffic competes with stores for store pipeline resources.

### 2.3 Managing FP/SIMD register contents
- Clear FP registers with zeroing idioms once results are consumed so physical entries can be reused and to avoid merge dependencies on partially updated registers.
- `XMM/YMM/ZMM` register-to-register moves have zero latency and can be used freely for data motion.
- Prefer register-destination forms of COMPRESS instructions; memory-destination forms are microcoded and can cap store bandwidth.
- In x87 code, `FXCH` is far faster than push/pop for swapping register stack entries; nothing should intervene between `FCOM` and `FSTSW` to keep comparisons precise.

### 2.4 MXCSR, denormals, and x87 FCW
- If IEEE-754 denormal precision is not required, set `MXCSR.DAZ` and `MXCSR.FTZ` to avoid severe denormal penalties (with semantic trade-offs).
- Denormal penalties depend on MXCSR configuration and instruction sequences; enabling both `DAZ` and `FTZ` avoids the penalties but deviates from strict IEEE-754 behavior.
- The x87 Floating-Point Control Word lacks DAZ/FTZ equivalents, so denormal penalties cannot be mitigated for x87 operations.

### 2.5 SIMD zeroing and ones idioms
- Common SIMD zeroing idioms: `VXORP(S/D)`, `VANDNP(S/D)`, `VPCMPGT(B/W/D/Q)`, `VPANDN`, `VPXOR`, `VPSUB(B/W/D/Q)`.
- Ones idioms: `PCMPEQ(B/W/D/Q)` for XMM and `VPCMPEQ(B/W/D/Q)` for ZMM/YMM/XMM.

### 2.6 XMM register merge and SSE/AVX mixing
- Zen4 can track when upper lanes of XMM are zero to avoid merge operations on those upper bits for some scalar instructions.
- Mixing SSE and AVX instructions can incur penalties when upper lanes hold non-zero data; avoid SSE/AVX mixing in hot code and use `vzeroupper` where required.

## 3. Physical register files and resource sharing

- The integer physical register file contains a large number of entries; under SMT, register pressure in one thread can impact the other.
- Flags are stored in a dedicated physical register file separate from the integer PRF.
- Under SMT, various queues and buffers are competitively shared between threads; be aware of contention (e.g., write-combining buffers).

## 4. SIMD, mask, and operand notation

- `mmx` for any 64-bit MMX register, `reg` for any GPR, `regN` for N-bit GPR, `xmmN`/`ymmN`/`zmmN` for SIMD regs, `k` for AVX-512 mask registers.
- `{k1}{z}` denotes AVX-512 zero masking; `{k1}` alone denotes merge masking.

## 5. System and MSR-related registers

### 5.1 PKRU, privileged reads, and shadow stack state
- Zen4 supports RDPRU along with RDPKRU/WRPKRU, enabling user-mode code to read privilege registers and to read/write PKRU where the ISA allows.
- Control-flow enforcement/shadow stack instructions manipulate SSP state (`INCSSP`, `RDSSP`, `SAVEPREVSSP`, `RSTORSSP`, `WRSS`, `WRUSS`, `SETSSBSY`, `CLRSSBSY`).

### 5.2 Prefetch control and cache-type registers
- Some server models implement a Prefetch Control MSR that can enable/disable prefetchers; CPUID enumerates the capability and PPR documents MSR fields.
- Zen4 uses MTRR and PAT to program ranges as WB/WP/WT/UC/WC; WC ranges allow merging writes in write-combining buffers.
- Write-combining regions must be configured via MTRR/PAT; refer to AMD system programming docs and PPR for exact register definitions.
- Streaming stores such as `MOVNTQ`/`MOVNTI` use write buffers and behave like WC regions, inheriting their flush rules.

### 5.3 Events that close the write-combining buffer
- IN/INS/OUT/OUTS treat memory as UC and close the WCB.
- Serializing instructions (e.g., MOVCRx, MOVDRx, WRMSR, INVLPG, CPUID, IRET, RSM, INIT, HALT) can flush the buffer.
- `CLFLUSH` closes WCB when memory type is WC or UC; cache/bus locks close buffers before the locked sequence.

### 5.4 LOCK guidance and debug MSRs
- For better LOCK behavior, keep locked accesses within 16-byte alignment, postpone FP instructions after locks, and consider debug/LBR settings if relevant.

