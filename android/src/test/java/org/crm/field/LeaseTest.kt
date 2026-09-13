package org.crm.field

import org.junit.Assert.*
import org.junit.Test

class LeaseTest {
    @Test
    fun sevenDaysCannotBeExtendedByClockRollbackOrReboot() {
        val lease = LeaseClock(8, 100, 604_800_000, 1_800_000_000_000)
        assertTrue(lease.usable(8, 604_800_099, 1_800_000_000_000))
        assertFalse(lease.usable(8, 604_800_100, 1_800_000_000_000))
        assertFalse(lease.usable(9, 110, 1_800_000_000_000))
        assertFalse(lease.usable(8, 99, 1_800_000_000_000))
        assertFalse(lease.usable(8, 110, 1_799_999_990_000))
        assertFalse(lease.copy(boot = -1).usable(-1, 110, 1_800_000_000_000))
    }

    @Test
    fun revisionOrderingDoesNotLosePrecision() {
        assertTrue(revisionAtLeast("9223372036854775808", "9223372036854775807"))
        assertFalse(revisionAtLeast("9", "10"))
        assertThrows(IllegalArgumentException::class.java) { revision("0") }
    }
}
