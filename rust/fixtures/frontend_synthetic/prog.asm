	.text
	.cpl_on
start:
	AC0 = *AR3
	AC1 = *AR3+
	AC2 = *AR4-
	AC0 = *+AR5
	AC0 = *-AR5
	AC0 = *AR3(#2)
	AC0 = *+AR3(#0x10)
	AC0 = *AR3(T0)
	AC1 = *(AR3+T0)
	AC1 = *(AR3-T1)
	AC0 = *CDP+
	AC0 = *(#0x1234)
	AC0 = *abs16(#0x10)
	AC0 = *SP(#3)
	.cpl_off
	T0 = @0x12
	T1 = @0x13 || mmap()
	AC0 = @0x1 || mmap()
	@0x3 = AR1 || mmap()
	.cpl_on
	AR3 = *port(#0x100)
	*port(#0x101) = AR3
	*AR4 = AC0 || writeport()
	AC0 = AC0 + (*AR3 << #16)
	AC1 = |AC0|
	AC2 = -AC1
	AC0 = AC0 & #0x00ff
	*AR0+ = HI(AC2 << T2)
	AC3 = (*AR0 << #16) - AC0 || *AR0+ = HI(AC2 << T2)
	AC0 = *AR0+ * *AR1+
	AC0 = AC0 + (*AR2 * coef(*CDP+)), AC1 = AC1 + (*AR3 * coef(*CDP+))
	AC0 = mant(AC1), T1 = exp(AC1)
	T0 = count(AC0, AC2, TC1)
	sqdst(*AR0, *AR1, AC0, AC1)
	HI(AC1) = HI(*AR0) + HI(AC0), LO(AC1) = LO(*AR0) + LO(AC0)
	HI(AC2) = HI(AC0) - HI(*AR1), LO(AC2) = LO(AC0) - LO(*AR1)
	subc(*AR1, AC0, AC0)
	bit(ST1, #9) = #1
	bit(ST0, #11) = #0
	TC1 = bit(*AR3, #2), bit(*AR3, #2) = #1
	push(AC0)
	push(T0,T1)
	pshboth(XAR5)
	dbl(push(AC1))
	push(dbl(*AR3))
	T0,T1 = pop()
	XAR5 = popboth()
	AC0 = dbl(pop())
	dbl(*AR3) = pop()
	mar(AR3 + T0)
	mar(AR2 - #3)
	mar(AR1 = #0x40)
	mar(*AR3+)
	XAR3 = mar(*(#0x2000))
	mar(*AR3+) || AC0 = AC0 + AR3
	mar(*AR4+) || AC0 = *AR4
	AC0 = dbl(*AR3)
	AC1 = uns(*AR3) * uns(*AR4)
	repeat(#5)
	AC0 = AC0 + *AR3+
	repeat(CSR)
	AC0 = AC0 + *AR3+
	localrepeat {
	AC0 = AC0 + *AR3+
	AC1 = AC1 + *AR4+
	}
	blockrepeat {
	AC0 = AC0 + *AR3+
	nop_16
	AC1 = AC1 + *AR4+
	}
	AC1 = AC0 || localrepeat {
	AC0 = AC0 + *AR3+
	AC1 = AC1 + *AR4+
	}
	localrepeat { || AC2 = AC0
	AC0 = AC0 + *AR3+
	AC1 = AC1 + *AR4+
	}
	AR3 = AR3 + #1 || blockrepeat {
	AC0 = AC0 + *AR3+
	AC1 = AC1 + *AR4+
	}
	while (TC1 && (RPTC < #5)) repeat
	subc(*AR1, AC0, AC0)
	if (AC0 != #0) execute(AD_Unit)
	AC1 = AC0
	if (AC0 == #0) execute(D_Unit)
	*AR3+ = AC1
	AC1 = AC0 || if (TC1) execute(AD_Unit)
	AC2 = AC0
	if (TC2) execute(D_Unit) || *AR3+ = AC1
	if (AC0 < #0) goto start
	compare (AR1 == #5) goto near
near:
	goto start
	call start
	call AC0
	goto AC1
	if (TC1) call start
	if (AC1 >= #0) return
	nop
	idle
	return_int
	return
