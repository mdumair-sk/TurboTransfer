package com.turbotransfer.presentation.components

import kotlin.math.max

/**
 * Lightweight, zero-dependency QR Code Matrix Generator.
 * Encodes alphanumeric / byte-mode strings (such as Wi-Fi credentials or URIs)
 * into a boolean 2D matrix suitable for direct Compose Canvas rendering.
 */
object QrCodeEncoder {

    // Galois Field GF(256) with primitive polynomial x^8 + x^4 + x^3 + x^2 + 1 (0x11D)
    private val expTable = IntArray(512)
    private val logTable = IntArray(256)

    init {
        var x = 1
        for (i in 0 until 255) {
            expTable[i] = x
            expTable[i + 255] = x
            logTable[x] = i
            x = (x shl 1)
            if (x >= 256) {
                x = x xor 0x11D
            }
        }
        logTable[0] = 0
    }

    private fun gfMul(x: Int, y: Int): Int {
        if (x == 0 || y == 0) return 0
        return expTable[logTable[x] + logTable[y]]
    }

    private fun rsGeneratorPoly(degree: Int): IntArray {
        var poly = intArrayOf(1)
        for (i in 0 until degree) {
            val factor = intArrayOf(1, expTable[i])
            val result = IntArray(poly.size + 1)
            for (p in poly.indices) {
                result[p] = result[p] xor poly[p]
                result[p + 1] = result[p + 1] xor gfMul(poly[p], factor[1])
            }
            poly = result
        }
        return poly
    }

    private fun rsEncode(data: IntArray, numEcBytes: Int): IntArray {
        val gen = rsGeneratorPoly(numEcBytes)
        val remainder = IntArray(numEcBytes)

        for (byteVal in data) {
            val factor = byteVal xor remainder[0]
            for (j in 0 until numEcBytes - 1) {
                remainder[j] = remainder[j + 1] xor gfMul(gen[j + 1], factor)
            }
            remainder[numEcBytes - 1] = gfMul(gen[numEcBytes], factor)
        }
        return remainder
    }

    data class VersionInfo(
        val version: Int,
        val size: Int,
        val totalDataCodewords: Int,
        val ecCodewordsPerBlock: Int,
        val numBlocks: Int,
        val alignmentPatterns: IntArray
    )

    // Standard QR Version tables for Error Correction Level M (15% redundancy)
    private val VERSIONS = listOf(
        VersionInfo(1, 21, 16, 10, 1, intArrayOf()),
        VersionInfo(2, 25, 28, 16, 1, intArrayOf(6, 18)),
        VersionInfo(3, 29, 44, 26, 1, intArrayOf(6, 22)),
        VersionInfo(4, 33, 64, 18, 2, intArrayOf(6, 26)),
        VersionInfo(5, 37, 86, 24, 2, intArrayOf(6, 30)),
        VersionInfo(6, 41, 108, 16, 4, intArrayOf(6, 34)),
        VersionInfo(7, 45, 124, 18, 4, intArrayOf(6, 22, 38)),
        VersionInfo(8, 49, 154, 22, 4, intArrayOf(6, 24, 42)),
        VersionInfo(9, 53, 182, 22, 5, intArrayOf(6, 26, 46)),
        VersionInfo(10, 57, 216, 26, 5, intArrayOf(6, 28, 50))
    )

    /**
     * Encodes [text] into a 2D boolean QR Matrix.
     * True = Dark module, False = Light module.
     */
    fun encode(text: String): Array<BooleanArray> {
        val rawBytes = text.toByteArray(Charsets.UTF_8)
        val dataLen = rawBytes.size

        val vInfo = VERSIONS.firstOrNull { it.totalDataCodewords - 2 >= dataLen }
            ?: throw IllegalArgumentException(
                "Text length ($dataLen bytes) exceeds maximum supported QR capacity (${VERSIONS.last().totalDataCodewords - 2} bytes)"
            )

        val totalDataBytes = vInfo.totalDataCodewords
        val bitBuffer = mutableListOf<Int>()

        fun appendBits(value: Int, count: Int) {
            for (i in count - 1 downTo 0) {
                bitBuffer.add((value shr i) and 1)
            }
        }

        // Byte Mode indicator = 0100 (4 bits)
        appendBits(4, 4)
        // Character count indicator (8 bits for versions 1-9, 16 bits for versions 10-40)
        val charCountBits = if (vInfo.version < 10) 8 else 16
        appendBits(dataLen, charCountBits)

        // Data payload
        for (b in rawBytes) {
            appendBits(b.toInt() and 0xFF, 8)
        }

        // Terminator bits (up to 4 zeroes)
        val remainingCapacityBits = totalDataBytes * 8 - bitBuffer.size
        val termBits = minOf(4, max(0, remainingCapacityBits))
        for (i in 0 until termBits) {
            bitBuffer.add(0)
        }

        // Pad to byte boundary
        while (bitBuffer.size % 8 != 0) {
            bitBuffer.add(0)
        }

        // Convert to byte codewords
        val dataCodewords = IntArray(totalDataBytes)
        val bitBytes = bitBuffer.size / 8
        for (i in 0 until minOf(bitBytes, totalDataBytes)) {
            var b = 0
            for (bit in 0 until 8) {
                b = (b shl 1) or bitBuffer[i * 8 + bit]
            }
            dataCodewords[i] = b
        }

        // Pad codewords (0xEC, 0x11 alternating)
        var padIndex = bitBytes
        var padVal = 0xEC
        while (padIndex < totalDataBytes) {
            dataCodewords[padIndex++] = padVal
            padVal = if (padVal == 0xEC) 0x11 else 0xEC
        }

        // Error Correction calculation supporting general multi-block division
        val numBlocks = vInfo.numBlocks
        val baseLen = totalDataBytes / numBlocks
        val extra = totalDataBytes % numBlocks
        val blockLengths = IntArray(numBlocks) { idx ->
            if (idx < numBlocks - extra) baseLen else baseLen + 1
        }
        var blockOffset = 0
        val dataBlocks = Array(numBlocks) { idx ->
            val len = blockLengths[idx]
            val slice = IntArray(len) { i -> dataCodewords[blockOffset + i] }
            blockOffset += len
            slice
        }
        val ecBlocks = Array(numBlocks) { idx ->
            rsEncode(dataBlocks[idx], vInfo.ecCodewordsPerBlock)
        }

        // Interleave data and EC codewords
        val maxBlockLen = (totalDataBytes + numBlocks - 1) / numBlocks
        val finalCodewords = mutableListOf<Int>()
        for (i in 0 until maxBlockLen) {
            for (b in 0 until numBlocks) {
                if (i < dataBlocks[b].size) {
                    finalCodewords.add(dataBlocks[b][i])
                }
            }
        }
        for (i in 0 until vInfo.ecCodewordsPerBlock) {
            for (b in 0 until numBlocks) {
                finalCodewords.add(ecBlocks[b][i])
            }
        }

        // Build matrix
        val size = vInfo.size
        val matrix = Array(size) { BooleanArray(size) }
        val isFunctionModule = Array(size) { BooleanArray(size) }

        fun setFunc(r: Int, c: Int, dark: Boolean) {
            if (r in 0 until size && c in 0 until size) {
                matrix[r][c] = dark
                isFunctionModule[r][c] = true
            }
        }

        // 1. Finder Patterns (Top-Left, Top-Right, Bottom-Left)
        fun placeFinder(top: Int, left: Int) {
            for (r in -1..7) {
                for (c in -1..7) {
                    val row = top + r
                    val col = left + c
                    if (row in 0 until size && col in 0 until size) {
                        val isBorder = r == -1 || r == 7 || c == -1 || c == 7
                        val isOuter = r == 0 || r == 6 || c == 0 || c == 6
                        val isInner = r in 2..4 && c in 2..4
                        setFunc(row, col, !isBorder && (isOuter || isInner))
                    }
                }
            }
        }

        placeFinder(0, 0)
        placeFinder(0, size - 7)
        placeFinder(size - 7, 0)

        // 2. Timing Patterns
        for (i in 8 until size - 8) {
            val dark = (i % 2 == 0)
            setFunc(6, i, dark)
            setFunc(i, 6, dark)
        }

        // 3. Alignment Patterns
        if (vInfo.alignmentPatterns.isNotEmpty()) {
            for (r in vInfo.alignmentPatterns) {
                for (c in vInfo.alignmentPatterns) {
                    if (isFunctionModule[r][c]) continue
                    for (dr in -2..2) {
                        for (dc in -2..2) {
                            val dark = dr == -2 || dr == 2 || dc == -2 || dc == 2 || (dr == 0 && dc == 0)
                            setFunc(r + dr, c + dc, dark)
                        }
                    }
                }
            }
        }

        // 4. Dark Module
        setFunc(size - 8, 8, true)

        // 5. Place Data with Mask 0 ((r + c) % 2 == 0)
        var bitIdx = 0
        val totalBits = finalCodewords.size * 8
        var col = size - 1

        while (col > 0) {
            if (col == 6) col--
            val upwards = ((size - 1 - col) / 2) % 2 == 0

            val rowRange = if (upwards) (size - 1 downTo 0) else (0 until size)
            for (row in rowRange) {
                for (cOffset in 0..1) {
                    val c = col - cOffset
                    if (!isFunctionModule[row][c]) {
                        val bit = if (bitIdx < totalBits) {
                            val byteVal = finalCodewords[bitIdx / 8]
                            (byteVal shr (7 - (bitIdx % 8))) and 1
                        } else 0
                        bitIdx++

                        val mask = (row + c) % 2 == 0
                        matrix[row][c] = (bit xor (if (mask) 1 else 0)) == 1
                    }
                }
            }
            col -= 2
        }

        // 6. Format Information (EC Level M = 00, Mask 0 = 000 -> 0x5412)
        val formatBits = 0x5412

        // Top-Left copy: bit 14 (MSB) to bit 0 (LSB) around finder
        for (i in 0 until 15) {
            val bit = ((formatBits shr (14 - i)) and 1) == 1
            if (i < 6) setFunc(8, i, bit)
            else if (i == 6) setFunc(8, 7, bit)
            else if (i == 7) setFunc(8, 8, bit)
            else if (i == 8) setFunc(7, 8, bit)
            else setFunc(14 - i, 8, bit)
        }

        // Bottom-Left copy: bit 0 (LSB) to bit 6
        for (i in 0 until 7) {
            val bit = ((formatBits shr i) and 1) == 1
            setFunc(size - 1 - i, 8, bit)
        }

        // Top-Right copy: bit 7 to bit 14 (MSB)
        for (i in 0 until 8) {
            val bit = ((formatBits shr (7 + i)) and 1) == 1
            setFunc(8, size - 8 + i, bit)
        }

        return matrix
    }
}
