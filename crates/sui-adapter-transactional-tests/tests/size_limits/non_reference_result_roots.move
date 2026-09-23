// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Stress test with a PTB whose commands return many plain non-reference values creates one result
// location per value.

//# init --addresses test=0x0 --accounts A

//# publish
module test::amplifier;

public fun r255(): (
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
) {
    (
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
}

//# programmable
//> 0: test::amplifier::r255();
//> 1: test::amplifier::r255();
//> 2: test::amplifier::r255();
//> 3: test::amplifier::r255();
//> 4: test::amplifier::r255();
//> 5: test::amplifier::r255();
//> 6: test::amplifier::r255();
//> 7: test::amplifier::r255();
//> 8: test::amplifier::r255();
//> 9: test::amplifier::r255();
//> 10: test::amplifier::r255();
//> 11: test::amplifier::r255();
//> 12: test::amplifier::r255();
//> 13: test::amplifier::r255();
//> 14: test::amplifier::r255();
//> 15: test::amplifier::r255();
//> 16: test::amplifier::r255();
//> 17: test::amplifier::r255();
//> 18: test::amplifier::r255();
//> 19: test::amplifier::r255();
//> 20: test::amplifier::r255();
//> 21: test::amplifier::r255();
//> 22: test::amplifier::r255();
//> 23: test::amplifier::r255();
//> 24: test::amplifier::r255();
//> 25: test::amplifier::r255();
//> 26: test::amplifier::r255();
//> 27: test::amplifier::r255();
//> 28: test::amplifier::r255();
//> 29: test::amplifier::r255();
//> 30: test::amplifier::r255();
//> 31: test::amplifier::r255();
//> 32: test::amplifier::r255();
//> 33: test::amplifier::r255();
//> 34: test::amplifier::r255();
//> 35: test::amplifier::r255();
//> 36: test::amplifier::r255();
//> 37: test::amplifier::r255();
//> 38: test::amplifier::r255();
//> 39: test::amplifier::r255();
//> 40: test::amplifier::r255();
//> 41: test::amplifier::r255();
//> 42: test::amplifier::r255();
//> 43: test::amplifier::r255();
//> 44: test::amplifier::r255();
//> 45: test::amplifier::r255();
//> 46: test::amplifier::r255();
//> 47: test::amplifier::r255();
//> 48: test::amplifier::r255();
//> 49: test::amplifier::r255();
//> 50: test::amplifier::r255();
//> 51: test::amplifier::r255();
//> 52: test::amplifier::r255();
//> 53: test::amplifier::r255();
//> 54: test::amplifier::r255();
//> 55: test::amplifier::r255();
//> 56: test::amplifier::r255();
//> 57: test::amplifier::r255();
//> 58: test::amplifier::r255();
//> 59: test::amplifier::r255();
//> 60: test::amplifier::r255();
//> 61: test::amplifier::r255();
//> 62: test::amplifier::r255();
//> 63: test::amplifier::r255();
//> 64: test::amplifier::r255();
//> 65: test::amplifier::r255();
//> 66: test::amplifier::r255();
//> 67: test::amplifier::r255();
//> 68: test::amplifier::r255();
//> 69: test::amplifier::r255();
//> 70: test::amplifier::r255();
//> 71: test::amplifier::r255();
//> 72: test::amplifier::r255();
//> 73: test::amplifier::r255();
//> 74: test::amplifier::r255();
//> 75: test::amplifier::r255();
//> 76: test::amplifier::r255();
//> 77: test::amplifier::r255();
//> 78: test::amplifier::r255();
//> 79: test::amplifier::r255();
//> 80: test::amplifier::r255();
//> 81: test::amplifier::r255();
//> 82: test::amplifier::r255();
//> 83: test::amplifier::r255();
//> 84: test::amplifier::r255();
//> 85: test::amplifier::r255();
//> 86: test::amplifier::r255();
//> 87: test::amplifier::r255();
//> 88: test::amplifier::r255();
//> 89: test::amplifier::r255();
//> 90: test::amplifier::r255();
//> 91: test::amplifier::r255();
//> 92: test::amplifier::r255();
//> 93: test::amplifier::r255();
//> 94: test::amplifier::r255();
//> 95: test::amplifier::r255();
//> 96: test::amplifier::r255();
//> 97: test::amplifier::r255();
//> 98: test::amplifier::r255();
//> 99: test::amplifier::r255();
//> 100: test::amplifier::r255();
//> 101: test::amplifier::r255();
//> 102: test::amplifier::r255();
//> 103: test::amplifier::r255();
//> 104: test::amplifier::r255();
//> 105: test::amplifier::r255();
//> 106: test::amplifier::r255();
//> 107: test::amplifier::r255();
//> 108: test::amplifier::r255();
//> 109: test::amplifier::r255();
//> 110: test::amplifier::r255();
//> 111: test::amplifier::r255();
//> 112: test::amplifier::r255();
//> 113: test::amplifier::r255();
//> 114: test::amplifier::r255();
//> 115: test::amplifier::r255();
//> 116: test::amplifier::r255();
//> 117: test::amplifier::r255();
//> 118: test::amplifier::r255();
//> 119: test::amplifier::r255();
//> 120: test::amplifier::r255();
//> 121: test::amplifier::r255();
//> 122: test::amplifier::r255();
//> 123: test::amplifier::r255();
//> 124: test::amplifier::r255();
//> 125: test::amplifier::r255();
//> 126: test::amplifier::r255();
//> 127: test::amplifier::r255();
//> 128: test::amplifier::r255();
//> 129: test::amplifier::r255();
//> 130: test::amplifier::r255();
//> 131: test::amplifier::r255();
//> 132: test::amplifier::r255();
//> 133: test::amplifier::r255();
//> 134: test::amplifier::r255();
//> 135: test::amplifier::r255();
//> 136: test::amplifier::r255();
//> 137: test::amplifier::r255();
//> 138: test::amplifier::r255();
//> 139: test::amplifier::r255();
//> 140: test::amplifier::r255();
//> 141: test::amplifier::r255();
//> 142: test::amplifier::r255();
//> 143: test::amplifier::r255();
//> 144: test::amplifier::r255();
//> 145: test::amplifier::r255();
//> 146: test::amplifier::r255();
//> 147: test::amplifier::r255();
//> 148: test::amplifier::r255();
//> 149: test::amplifier::r255();
//> 150: test::amplifier::r255();
//> 151: test::amplifier::r255();
//> 152: test::amplifier::r255();
//> 153: test::amplifier::r255();
//> 154: test::amplifier::r255();
//> 155: test::amplifier::r255();
//> 156: test::amplifier::r255();
//> 157: test::amplifier::r255();
//> 158: test::amplifier::r255();
//> 159: test::amplifier::r255();
//> 160: test::amplifier::r255();
//> 161: test::amplifier::r255();
//> 162: test::amplifier::r255();
//> 163: test::amplifier::r255();
//> 164: test::amplifier::r255();
//> 165: test::amplifier::r255();
//> 166: test::amplifier::r255();
//> 167: test::amplifier::r255();
//> 168: test::amplifier::r255();
//> 169: test::amplifier::r255();
//> 170: test::amplifier::r255();
//> 171: test::amplifier::r255();
//> 172: test::amplifier::r255();
//> 173: test::amplifier::r255();
//> 174: test::amplifier::r255();
//> 175: test::amplifier::r255();
//> 176: test::amplifier::r255();
//> 177: test::amplifier::r255();
//> 178: test::amplifier::r255();
//> 179: test::amplifier::r255();
//> 180: test::amplifier::r255();
//> 181: test::amplifier::r255();
//> 182: test::amplifier::r255();
//> 183: test::amplifier::r255();
//> 184: test::amplifier::r255();
//> 185: test::amplifier::r255();
//> 186: test::amplifier::r255();
//> 187: test::amplifier::r255();
//> 188: test::amplifier::r255();
//> 189: test::amplifier::r255();
//> 190: test::amplifier::r255();
//> 191: test::amplifier::r255();
//> 192: test::amplifier::r255();
//> 193: test::amplifier::r255();
//> 194: test::amplifier::r255();
//> 195: test::amplifier::r255();
//> 196: test::amplifier::r255();
//> 197: test::amplifier::r255();
//> 198: test::amplifier::r255();
//> 199: test::amplifier::r255();
//> 200: test::amplifier::r255();
//> 201: test::amplifier::r255();
//> 202: test::amplifier::r255();
//> 203: test::amplifier::r255();
//> 204: test::amplifier::r255();
//> 205: test::amplifier::r255();
//> 206: test::amplifier::r255();
//> 207: test::amplifier::r255();
//> 208: test::amplifier::r255();
//> 209: test::amplifier::r255();
//> 210: test::amplifier::r255();
//> 211: test::amplifier::r255();
//> 212: test::amplifier::r255();
//> 213: test::amplifier::r255();
//> 214: test::amplifier::r255();
//> 215: test::amplifier::r255();
//> 216: test::amplifier::r255();
//> 217: test::amplifier::r255();
//> 218: test::amplifier::r255();
//> 219: test::amplifier::r255();
//> 220: test::amplifier::r255();
//> 221: test::amplifier::r255();
//> 222: test::amplifier::r255();
//> 223: test::amplifier::r255();
//> 224: test::amplifier::r255();
//> 225: test::amplifier::r255();
//> 226: test::amplifier::r255();
//> 227: test::amplifier::r255();
//> 228: test::amplifier::r255();
//> 229: test::amplifier::r255();
//> 230: test::amplifier::r255();
//> 231: test::amplifier::r255();
//> 232: test::amplifier::r255();
//> 233: test::amplifier::r255();
//> 234: test::amplifier::r255();
//> 235: test::amplifier::r255();
//> 236: test::amplifier::r255();
//> 237: test::amplifier::r255();
//> 238: test::amplifier::r255();
//> 239: test::amplifier::r255();
//> 240: test::amplifier::r255();
//> 241: test::amplifier::r255();
//> 242: test::amplifier::r255();
//> 243: test::amplifier::r255();
//> 244: test::amplifier::r255();
//> 245: test::amplifier::r255();
//> 246: test::amplifier::r255();
//> 247: test::amplifier::r255();
//> 248: test::amplifier::r255();
//> 249: test::amplifier::r255();
//> 250: test::amplifier::r255();
//> 251: test::amplifier::r255();
//> 252: test::amplifier::r255();
//> 253: test::amplifier::r255();
//> 254: test::amplifier::r255();
//> 255: test::amplifier::r255();
//> 256: test::amplifier::r255();
//> 257: test::amplifier::r255();
//> 258: test::amplifier::r255();
//> 259: test::amplifier::r255();
//> 260: test::amplifier::r255();
//> 261: test::amplifier::r255();
//> 262: test::amplifier::r255();
//> 263: test::amplifier::r255();
//> 264: test::amplifier::r255();
//> 265: test::amplifier::r255();
//> 266: test::amplifier::r255();
//> 267: test::amplifier::r255();
//> 268: test::amplifier::r255();
//> 269: test::amplifier::r255();
//> 270: test::amplifier::r255();
//> 271: test::amplifier::r255();
//> 272: test::amplifier::r255();
//> 273: test::amplifier::r255();
//> 274: test::amplifier::r255();
//> 275: test::amplifier::r255();
//> 276: test::amplifier::r255();
//> 277: test::amplifier::r255();
//> 278: test::amplifier::r255();
//> 279: test::amplifier::r255();
//> 280: test::amplifier::r255();
//> 281: test::amplifier::r255();
//> 282: test::amplifier::r255();
//> 283: test::amplifier::r255();
//> 284: test::amplifier::r255();
//> 285: test::amplifier::r255();
//> 286: test::amplifier::r255();
//> 287: test::amplifier::r255();
//> 288: test::amplifier::r255();
//> 289: test::amplifier::r255();
//> 290: test::amplifier::r255();
//> 291: test::amplifier::r255();
//> 292: test::amplifier::r255();
//> 293: test::amplifier::r255();
//> 294: test::amplifier::r255();
//> 295: test::amplifier::r255();
//> 296: test::amplifier::r255();
//> 297: test::amplifier::r255();
//> 298: test::amplifier::r255();
//> 299: test::amplifier::r255();
//> 300: test::amplifier::r255();
//> 301: test::amplifier::r255();
//> 302: test::amplifier::r255();
//> 303: test::amplifier::r255();
//> 304: test::amplifier::r255();
//> 305: test::amplifier::r255();
//> 306: test::amplifier::r255();
//> 307: test::amplifier::r255();
//> 308: test::amplifier::r255();
//> 309: test::amplifier::r255();
//> 310: test::amplifier::r255();
//> 311: test::amplifier::r255();
//> 312: test::amplifier::r255();
//> 313: test::amplifier::r255();
//> 314: test::amplifier::r255();
//> 315: test::amplifier::r255();
//> 316: test::amplifier::r255();
//> 317: test::amplifier::r255();
//> 318: test::amplifier::r255();
//> 319: test::amplifier::r255();
//> 320: test::amplifier::r255();
//> 321: test::amplifier::r255();
//> 322: test::amplifier::r255();
//> 323: test::amplifier::r255();
//> 324: test::amplifier::r255();
//> 325: test::amplifier::r255();
//> 326: test::amplifier::r255();
//> 327: test::amplifier::r255();
//> 328: test::amplifier::r255();
//> 329: test::amplifier::r255();
//> 330: test::amplifier::r255();
//> 331: test::amplifier::r255();
//> 332: test::amplifier::r255();
//> 333: test::amplifier::r255();
//> 334: test::amplifier::r255();
//> 335: test::amplifier::r255();
//> 336: test::amplifier::r255();
//> 337: test::amplifier::r255();
//> 338: test::amplifier::r255();
//> 339: test::amplifier::r255();
//> 340: test::amplifier::r255();
//> 341: test::amplifier::r255();
//> 342: test::amplifier::r255();
//> 343: test::amplifier::r255();
//> 344: test::amplifier::r255();
//> 345: test::amplifier::r255();
//> 346: test::amplifier::r255();
//> 347: test::amplifier::r255();
//> 348: test::amplifier::r255();
//> 349: test::amplifier::r255();
//> 350: test::amplifier::r255();
//> 351: test::amplifier::r255();
//> 352: test::amplifier::r255();
//> 353: test::amplifier::r255();
//> 354: test::amplifier::r255();
//> 355: test::amplifier::r255();
//> 356: test::amplifier::r255();
//> 357: test::amplifier::r255();
//> 358: test::amplifier::r255();
//> 359: test::amplifier::r255();
//> 360: test::amplifier::r255();
//> 361: test::amplifier::r255();
//> 362: test::amplifier::r255();
//> 363: test::amplifier::r255();
//> 364: test::amplifier::r255();
//> 365: test::amplifier::r255();
//> 366: test::amplifier::r255();
//> 367: test::amplifier::r255();
//> 368: test::amplifier::r255();
//> 369: test::amplifier::r255();
//> 370: test::amplifier::r255();
//> 371: test::amplifier::r255();
//> 372: test::amplifier::r255();
//> 373: test::amplifier::r255();
//> 374: test::amplifier::r255();
//> 375: test::amplifier::r255();
//> 376: test::amplifier::r255();
//> 377: test::amplifier::r255();
//> 378: test::amplifier::r255();
//> 379: test::amplifier::r255();
//> 380: test::amplifier::r255();
//> 381: test::amplifier::r255();
//> 382: test::amplifier::r255();
//> 383: test::amplifier::r255();
//> 384: test::amplifier::r255();
//> 385: test::amplifier::r255();
//> 386: test::amplifier::r255();
//> 387: test::amplifier::r255();
//> 388: test::amplifier::r255();
//> 389: test::amplifier::r255();
//> 390: test::amplifier::r255();
//> 391: test::amplifier::r255();
//> 392: test::amplifier::r255();
//> 393: test::amplifier::r255();
//> 394: test::amplifier::r255();
//> 395: test::amplifier::r255();
//> 396: test::amplifier::r255();
//> 397: test::amplifier::r255();
//> 398: test::amplifier::r255();
//> 399: test::amplifier::r255();
//> 400: test::amplifier::r255();
//> 401: test::amplifier::r255();
//> 402: test::amplifier::r255();
//> 403: test::amplifier::r255();
//> 404: test::amplifier::r255();
//> 405: test::amplifier::r255();
//> 406: test::amplifier::r255();
//> 407: test::amplifier::r255();
//> 408: test::amplifier::r255();
//> 409: test::amplifier::r255();
//> 410: test::amplifier::r255();
//> 411: test::amplifier::r255();
//> 412: test::amplifier::r255();
//> 413: test::amplifier::r255();
//> 414: test::amplifier::r255();
//> 415: test::amplifier::r255();
//> 416: test::amplifier::r255();
//> 417: test::amplifier::r255();
//> 418: test::amplifier::r255();
//> 419: test::amplifier::r255();
//> 420: test::amplifier::r255();
//> 421: test::amplifier::r255();
//> 422: test::amplifier::r255();
//> 423: test::amplifier::r255();
//> 424: test::amplifier::r255();
//> 425: test::amplifier::r255();
//> 426: test::amplifier::r255();
//> 427: test::amplifier::r255();
//> 428: test::amplifier::r255();
//> 429: test::amplifier::r255();
//> 430: test::amplifier::r255();
//> 431: test::amplifier::r255();
//> 432: test::amplifier::r255();
//> 433: test::amplifier::r255();
//> 434: test::amplifier::r255();
//> 435: test::amplifier::r255();
//> 436: test::amplifier::r255();
//> 437: test::amplifier::r255();
//> 438: test::amplifier::r255();
//> 439: test::amplifier::r255();
//> 440: test::amplifier::r255();
//> 441: test::amplifier::r255();
//> 442: test::amplifier::r255();
//> 443: test::amplifier::r255();
//> 444: test::amplifier::r255();
//> 445: test::amplifier::r255();
//> 446: test::amplifier::r255();
//> 447: test::amplifier::r255();
//> 448: test::amplifier::r255();
//> 449: test::amplifier::r255();
//> 450: test::amplifier::r255();
//> 451: test::amplifier::r255();
//> 452: test::amplifier::r255();
//> 453: test::amplifier::r255();
//> 454: test::amplifier::r255();
//> 455: test::amplifier::r255();
//> 456: test::amplifier::r255();
//> 457: test::amplifier::r255();
//> 458: test::amplifier::r255();
//> 459: test::amplifier::r255();
//> 460: test::amplifier::r255();
//> 461: test::amplifier::r255();
//> 462: test::amplifier::r255();
//> 463: test::amplifier::r255();
//> 464: test::amplifier::r255();
//> 465: test::amplifier::r255();
//> 466: test::amplifier::r255();
//> 467: test::amplifier::r255();
//> 468: test::amplifier::r255();
//> 469: test::amplifier::r255();
//> 470: test::amplifier::r255();
//> 471: test::amplifier::r255();
//> 472: test::amplifier::r255();
//> 473: test::amplifier::r255();
//> 474: test::amplifier::r255();
//> 475: test::amplifier::r255();
//> 476: test::amplifier::r255();
//> 477: test::amplifier::r255();
//> 478: test::amplifier::r255();
//> 479: test::amplifier::r255();
//> 480: test::amplifier::r255();
//> 481: test::amplifier::r255();
//> 482: test::amplifier::r255();
//> 483: test::amplifier::r255();
//> 484: test::amplifier::r255();
//> 485: test::amplifier::r255();
//> 486: test::amplifier::r255();
//> 487: test::amplifier::r255();
//> 488: test::amplifier::r255();
//> 489: test::amplifier::r255();
//> 490: test::amplifier::r255();
//> 491: test::amplifier::r255();
//> 492: test::amplifier::r255();
//> 493: test::amplifier::r255();
//> 494: test::amplifier::r255();
//> 495: test::amplifier::r255();
//> 496: test::amplifier::r255();
//> 497: test::amplifier::r255();
//> 498: test::amplifier::r255();
//> 499: test::amplifier::r255();
//> 500: test::amplifier::r255();
//> 501: test::amplifier::r255();
//> 502: test::amplifier::r255();
//> 503: test::amplifier::r255();
//> 504: test::amplifier::r255();
//> 505: test::amplifier::r255();
//> 506: test::amplifier::r255();
//> 507: test::amplifier::r255();
//> 508: test::amplifier::r255();
//> 509: test::amplifier::r255();
//> 510: test::amplifier::r255();
//> 511: test::amplifier::r255();
//> 512: test::amplifier::r255();
//> 513: test::amplifier::r255();
//> 514: test::amplifier::r255();
//> 515: test::amplifier::r255();
//> 516: test::amplifier::r255();
//> 517: test::amplifier::r255();
//> 518: test::amplifier::r255();
//> 519: test::amplifier::r255();
//> 520: test::amplifier::r255();
//> 521: test::amplifier::r255();
//> 522: test::amplifier::r255();
//> 523: test::amplifier::r255();
//> 524: test::amplifier::r255();
//> 525: test::amplifier::r255();
//> 526: test::amplifier::r255();
//> 527: test::amplifier::r255();
//> 528: test::amplifier::r255();
//> 529: test::amplifier::r255();
//> 530: test::amplifier::r255();
//> 531: test::amplifier::r255();
//> 532: test::amplifier::r255();
//> 533: test::amplifier::r255();
//> 534: test::amplifier::r255();
//> 535: test::amplifier::r255();
//> 536: test::amplifier::r255();
//> 537: test::amplifier::r255();
//> 538: test::amplifier::r255();
//> 539: test::amplifier::r255();
//> 540: test::amplifier::r255();
//> 541: test::amplifier::r255();
//> 542: test::amplifier::r255();
//> 543: test::amplifier::r255();
//> 544: test::amplifier::r255();
//> 545: test::amplifier::r255();
//> 546: test::amplifier::r255();
//> 547: test::amplifier::r255();
//> 548: test::amplifier::r255();
//> 549: test::amplifier::r255();
//> 550: test::amplifier::r255();
//> 551: test::amplifier::r255();
//> 552: test::amplifier::r255();
//> 553: test::amplifier::r255();
//> 554: test::amplifier::r255();
//> 555: test::amplifier::r255();
//> 556: test::amplifier::r255();
//> 557: test::amplifier::r255();
//> 558: test::amplifier::r255();
//> 559: test::amplifier::r255();
//> 560: test::amplifier::r255();
//> 561: test::amplifier::r255();
//> 562: test::amplifier::r255();
//> 563: test::amplifier::r255();
//> 564: test::amplifier::r255();
//> 565: test::amplifier::r255();
//> 566: test::amplifier::r255();
//> 567: test::amplifier::r255();
//> 568: test::amplifier::r255();
//> 569: test::amplifier::r255();
//> 570: test::amplifier::r255();
//> 571: test::amplifier::r255();
//> 572: test::amplifier::r255();
//> 573: test::amplifier::r255();
//> 574: test::amplifier::r255();
//> 575: test::amplifier::r255();
//> 576: test::amplifier::r255();
//> 577: test::amplifier::r255();
//> 578: test::amplifier::r255();
//> 579: test::amplifier::r255();
//> 580: test::amplifier::r255();
//> 581: test::amplifier::r255();
//> 582: test::amplifier::r255();
//> 583: test::amplifier::r255();
//> 584: test::amplifier::r255();
//> 585: test::amplifier::r255();
//> 586: test::amplifier::r255();
//> 587: test::amplifier::r255();
//> 588: test::amplifier::r255();
//> 589: test::amplifier::r255();
//> 590: test::amplifier::r255();
//> 591: test::amplifier::r255();
//> 592: test::amplifier::r255();
//> 593: test::amplifier::r255();
//> 594: test::amplifier::r255();
//> 595: test::amplifier::r255();
//> 596: test::amplifier::r255();
//> 597: test::amplifier::r255();
//> 598: test::amplifier::r255();
//> 599: test::amplifier::r255();
//> 600: test::amplifier::r255();
//> 601: test::amplifier::r255();
//> 602: test::amplifier::r255();
//> 603: test::amplifier::r255();
//> 604: test::amplifier::r255();
//> 605: test::amplifier::r255();
//> 606: test::amplifier::r255();
//> 607: test::amplifier::r255();
//> 608: test::amplifier::r255();
//> 609: test::amplifier::r255();
//> 610: test::amplifier::r255();
//> 611: test::amplifier::r255();
//> 612: test::amplifier::r255();
//> 613: test::amplifier::r255();
//> 614: test::amplifier::r255();
//> 615: test::amplifier::r255();
//> 616: test::amplifier::r255();
//> 617: test::amplifier::r255();
//> 618: test::amplifier::r255();
//> 619: test::amplifier::r255();
//> 620: test::amplifier::r255();
//> 621: test::amplifier::r255();
//> 622: test::amplifier::r255();
//> 623: test::amplifier::r255();
//> 624: test::amplifier::r255();
//> 625: test::amplifier::r255();
//> 626: test::amplifier::r255();
//> 627: test::amplifier::r255();
//> 628: test::amplifier::r255();
//> 629: test::amplifier::r255();
//> 630: test::amplifier::r255();
//> 631: test::amplifier::r255();
//> 632: test::amplifier::r255();
//> 633: test::amplifier::r255();
//> 634: test::amplifier::r255();
//> 635: test::amplifier::r255();
//> 636: test::amplifier::r255();
//> 637: test::amplifier::r255();
//> 638: test::amplifier::r255();
//> 639: test::amplifier::r255();
//> 640: test::amplifier::r255();
//> 641: test::amplifier::r255();
//> 642: test::amplifier::r255();
//> 643: test::amplifier::r255();
//> 644: test::amplifier::r255();
//> 645: test::amplifier::r255();
//> 646: test::amplifier::r255();
//> 647: test::amplifier::r255();
//> 648: test::amplifier::r255();
//> 649: test::amplifier::r255();
//> 650: test::amplifier::r255();
//> 651: test::amplifier::r255();
//> 652: test::amplifier::r255();
//> 653: test::amplifier::r255();
//> 654: test::amplifier::r255();
//> 655: test::amplifier::r255();
//> 656: test::amplifier::r255();
//> 657: test::amplifier::r255();
//> 658: test::amplifier::r255();
//> 659: test::amplifier::r255();
//> 660: test::amplifier::r255();
//> 661: test::amplifier::r255();
//> 662: test::amplifier::r255();
//> 663: test::amplifier::r255();
//> 664: test::amplifier::r255();
//> 665: test::amplifier::r255();
//> 666: test::amplifier::r255();
//> 667: test::amplifier::r255();
//> 668: test::amplifier::r255();
//> 669: test::amplifier::r255();
//> 670: test::amplifier::r255();
//> 671: test::amplifier::r255();
//> 672: test::amplifier::r255();
//> 673: test::amplifier::r255();
//> 674: test::amplifier::r255();
//> 675: test::amplifier::r255();
//> 676: test::amplifier::r255();
//> 677: test::amplifier::r255();
//> 678: test::amplifier::r255();
//> 679: test::amplifier::r255();
//> 680: test::amplifier::r255();
//> 681: test::amplifier::r255();
//> 682: test::amplifier::r255();
//> 683: test::amplifier::r255();
//> 684: test::amplifier::r255();
//> 685: test::amplifier::r255();
//> 686: test::amplifier::r255();
//> 687: test::amplifier::r255();
//> 688: test::amplifier::r255();
//> 689: test::amplifier::r255();
//> 690: test::amplifier::r255();
//> 691: test::amplifier::r255();
//> 692: test::amplifier::r255();
//> 693: test::amplifier::r255();
//> 694: test::amplifier::r255();
//> 695: test::amplifier::r255();
//> 696: test::amplifier::r255();
//> 697: test::amplifier::r255();
//> 698: test::amplifier::r255();
//> 699: test::amplifier::r255();
//> 700: test::amplifier::r255();
//> 701: test::amplifier::r255();
//> 702: test::amplifier::r255();
//> 703: test::amplifier::r255();
//> 704: test::amplifier::r255();
//> 705: test::amplifier::r255();
//> 706: test::amplifier::r255();
//> 707: test::amplifier::r255();
//> 708: test::amplifier::r255();
//> 709: test::amplifier::r255();
//> 710: test::amplifier::r255();
//> 711: test::amplifier::r255();
//> 712: test::amplifier::r255();
//> 713: test::amplifier::r255();
//> 714: test::amplifier::r255();
//> 715: test::amplifier::r255();
//> 716: test::amplifier::r255();
//> 717: test::amplifier::r255();
//> 718: test::amplifier::r255();
//> 719: test::amplifier::r255();
//> 720: test::amplifier::r255();
//> 721: test::amplifier::r255();
//> 722: test::amplifier::r255();
//> 723: test::amplifier::r255();
//> 724: test::amplifier::r255();
//> 725: test::amplifier::r255();
//> 726: test::amplifier::r255();
//> 727: test::amplifier::r255();
//> 728: test::amplifier::r255();
//> 729: test::amplifier::r255();
//> 730: test::amplifier::r255();
//> 731: test::amplifier::r255();
//> 732: test::amplifier::r255();
//> 733: test::amplifier::r255();
//> 734: test::amplifier::r255();
//> 735: test::amplifier::r255();
//> 736: test::amplifier::r255();
//> 737: test::amplifier::r255();
//> 738: test::amplifier::r255();
//> 739: test::amplifier::r255();
//> 740: test::amplifier::r255();
//> 741: test::amplifier::r255();
//> 742: test::amplifier::r255();
//> 743: test::amplifier::r255();
//> 744: test::amplifier::r255();
//> 745: test::amplifier::r255();
//> 746: test::amplifier::r255();
//> 747: test::amplifier::r255();
//> 748: test::amplifier::r255();
//> 749: test::amplifier::r255();
//> 750: test::amplifier::r255();
//> 751: test::amplifier::r255();
//> 752: test::amplifier::r255();
//> 753: test::amplifier::r255();
//> 754: test::amplifier::r255();
//> 755: test::amplifier::r255();
//> 756: test::amplifier::r255();
//> 757: test::amplifier::r255();
//> 758: test::amplifier::r255();
//> 759: test::amplifier::r255();
//> 760: test::amplifier::r255();
//> 761: test::amplifier::r255();
//> 762: test::amplifier::r255();
//> 763: test::amplifier::r255();
//> 764: test::amplifier::r255();
//> 765: test::amplifier::r255();
//> 766: test::amplifier::r255();
//> 767: test::amplifier::r255();
//> 768: test::amplifier::r255();
//> 769: test::amplifier::r255();
//> 770: test::amplifier::r255();
//> 771: test::amplifier::r255();
//> 772: test::amplifier::r255();
//> 773: test::amplifier::r255();
//> 774: test::amplifier::r255();
//> 775: test::amplifier::r255();
//> 776: test::amplifier::r255();
//> 777: test::amplifier::r255();
//> 778: test::amplifier::r255();
//> 779: test::amplifier::r255();
//> 780: test::amplifier::r255();
//> 781: test::amplifier::r255();
//> 782: test::amplifier::r255();
//> 783: test::amplifier::r255();
//> 784: test::amplifier::r255();
//> 785: test::amplifier::r255();
//> 786: test::amplifier::r255();
//> 787: test::amplifier::r255();
//> 788: test::amplifier::r255();
//> 789: test::amplifier::r255();
//> 790: test::amplifier::r255();
//> 791: test::amplifier::r255();
//> 792: test::amplifier::r255();
//> 793: test::amplifier::r255();
//> 794: test::amplifier::r255();
//> 795: test::amplifier::r255();
//> 796: test::amplifier::r255();
//> 797: test::amplifier::r255();
//> 798: test::amplifier::r255();
//> 799: test::amplifier::r255();
//> 800: test::amplifier::r255();
//> 801: test::amplifier::r255();
//> 802: test::amplifier::r255();
//> 803: test::amplifier::r255();
//> 804: test::amplifier::r255();
//> 805: test::amplifier::r255();
//> 806: test::amplifier::r255();
//> 807: test::amplifier::r255();
//> 808: test::amplifier::r255();
//> 809: test::amplifier::r255();
//> 810: test::amplifier::r255();
//> 811: test::amplifier::r255();
//> 812: test::amplifier::r255();
//> 813: test::amplifier::r255();
//> 814: test::amplifier::r255();
//> 815: test::amplifier::r255();
//> 816: test::amplifier::r255();
//> 817: test::amplifier::r255();
//> 818: test::amplifier::r255();
//> 819: test::amplifier::r255();
//> 820: test::amplifier::r255();
//> 821: test::amplifier::r255();
//> 822: test::amplifier::r255();
//> 823: test::amplifier::r255();
//> 824: test::amplifier::r255();
//> 825: test::amplifier::r255();
//> 826: test::amplifier::r255();
//> 827: test::amplifier::r255();
//> 828: test::amplifier::r255();
//> 829: test::amplifier::r255();
//> 830: test::amplifier::r255();
//> 831: test::amplifier::r255();
//> 832: test::amplifier::r255();
//> 833: test::amplifier::r255();
//> 834: test::amplifier::r255();
//> 835: test::amplifier::r255();
//> 836: test::amplifier::r255();
//> 837: test::amplifier::r255();
//> 838: test::amplifier::r255();
//> 839: test::amplifier::r255();
//> 840: test::amplifier::r255();
//> 841: test::amplifier::r255();
//> 842: test::amplifier::r255();
//> 843: test::amplifier::r255();
//> 844: test::amplifier::r255();
//> 845: test::amplifier::r255();
//> 846: test::amplifier::r255();
//> 847: test::amplifier::r255();
//> 848: test::amplifier::r255();
//> 849: test::amplifier::r255();
//> 850: test::amplifier::r255();
//> 851: test::amplifier::r255();
//> 852: test::amplifier::r255();
//> 853: test::amplifier::r255();
//> 854: test::amplifier::r255();
//> 855: test::amplifier::r255();
//> 856: test::amplifier::r255();
//> 857: test::amplifier::r255();
//> 858: test::amplifier::r255();
//> 859: test::amplifier::r255();
//> 860: test::amplifier::r255();
//> 861: test::amplifier::r255();
//> 862: test::amplifier::r255();
//> 863: test::amplifier::r255();
//> 864: test::amplifier::r255();
//> 865: test::amplifier::r255();
//> 866: test::amplifier::r255();
//> 867: test::amplifier::r255();
//> 868: test::amplifier::r255();
//> 869: test::amplifier::r255();
//> 870: test::amplifier::r255();
//> 871: test::amplifier::r255();
//> 872: test::amplifier::r255();
//> 873: test::amplifier::r255();
//> 874: test::amplifier::r255();
//> 875: test::amplifier::r255();
//> 876: test::amplifier::r255();
//> 877: test::amplifier::r255();
//> 878: test::amplifier::r255();
//> 879: test::amplifier::r255();
//> 880: test::amplifier::r255();
//> 881: test::amplifier::r255();
//> 882: test::amplifier::r255();
//> 883: test::amplifier::r255();
//> 884: test::amplifier::r255();
//> 885: test::amplifier::r255();
//> 886: test::amplifier::r255();
//> 887: test::amplifier::r255();
//> 888: test::amplifier::r255();
//> 889: test::amplifier::r255();
//> 890: test::amplifier::r255();
//> 891: test::amplifier::r255();
//> 892: test::amplifier::r255();
//> 893: test::amplifier::r255();
//> 894: test::amplifier::r255();
//> 895: test::amplifier::r255();
//> 896: test::amplifier::r255();
//> 897: test::amplifier::r255();
//> 898: test::amplifier::r255();
//> 899: test::amplifier::r255();
//> 900: test::amplifier::r255();
//> 901: test::amplifier::r255();
//> 902: test::amplifier::r255();
//> 903: test::amplifier::r255();
//> 904: test::amplifier::r255();
//> 905: test::amplifier::r255();
//> 906: test::amplifier::r255();
//> 907: test::amplifier::r255();
//> 908: test::amplifier::r255();
//> 909: test::amplifier::r255();
//> 910: test::amplifier::r255();
//> 911: test::amplifier::r255();
//> 912: test::amplifier::r255();
//> 913: test::amplifier::r255();
//> 914: test::amplifier::r255();
//> 915: test::amplifier::r255();
//> 916: test::amplifier::r255();
//> 917: test::amplifier::r255();
//> 918: test::amplifier::r255();
//> 919: test::amplifier::r255();
//> 920: test::amplifier::r255();
//> 921: test::amplifier::r255();
//> 922: test::amplifier::r255();
//> 923: test::amplifier::r255();
//> 924: test::amplifier::r255();
//> 925: test::amplifier::r255();
//> 926: test::amplifier::r255();
//> 927: test::amplifier::r255();
//> 928: test::amplifier::r255();
//> 929: test::amplifier::r255();
//> 930: test::amplifier::r255();
//> 931: test::amplifier::r255();
//> 932: test::amplifier::r255();
//> 933: test::amplifier::r255();
//> 934: test::amplifier::r255();
//> 935: test::amplifier::r255();
//> 936: test::amplifier::r255();
//> 937: test::amplifier::r255();
//> 938: test::amplifier::r255();
//> 939: test::amplifier::r255();
//> 940: test::amplifier::r255();
//> 941: test::amplifier::r255();
//> 942: test::amplifier::r255();
//> 943: test::amplifier::r255();
//> 944: test::amplifier::r255();
//> 945: test::amplifier::r255();
//> 946: test::amplifier::r255();
//> 947: test::amplifier::r255();
//> 948: test::amplifier::r255();
//> 949: test::amplifier::r255();
//> 950: test::amplifier::r255();
//> 951: test::amplifier::r255();
//> 952: test::amplifier::r255();
//> 953: test::amplifier::r255();
//> 954: test::amplifier::r255();
//> 955: test::amplifier::r255();
//> 956: test::amplifier::r255();
//> 957: test::amplifier::r255();
//> 958: test::amplifier::r255();
//> 959: test::amplifier::r255();
//> 960: test::amplifier::r255();
//> 961: test::amplifier::r255();
//> 962: test::amplifier::r255();
//> 963: test::amplifier::r255();
//> 964: test::amplifier::r255();
//> 965: test::amplifier::r255();
//> 966: test::amplifier::r255();
//> 967: test::amplifier::r255();
//> 968: test::amplifier::r255();
//> 969: test::amplifier::r255();
//> 970: test::amplifier::r255();
//> 971: test::amplifier::r255();
//> 972: test::amplifier::r255();
//> 973: test::amplifier::r255();
//> 974: test::amplifier::r255();
//> 975: test::amplifier::r255();
//> 976: test::amplifier::r255();
//> 977: test::amplifier::r255();
//> 978: test::amplifier::r255();
//> 979: test::amplifier::r255();
//> 980: test::amplifier::r255();
//> 981: test::amplifier::r255();
//> 982: test::amplifier::r255();
//> 983: test::amplifier::r255();
//> 984: test::amplifier::r255();
//> 985: test::amplifier::r255();
//> 986: test::amplifier::r255();
//> 987: test::amplifier::r255();
//> 988: test::amplifier::r255();
//> 989: test::amplifier::r255();
//> 990: test::amplifier::r255();
//> 991: test::amplifier::r255();
//> 992: test::amplifier::r255();
//> 993: test::amplifier::r255();
//> 994: test::amplifier::r255();
//> 995: test::amplifier::r255();
//> 996: test::amplifier::r255();
//> 997: test::amplifier::r255();
//> 998: test::amplifier::r255();
//> 999: test::amplifier::r255();
//> 1000: test::amplifier::r255();
//> 1001: test::amplifier::r255();
//> 1002: test::amplifier::r255();
//> 1003: test::amplifier::r255();
//> 1004: test::amplifier::r255();
//> 1005: test::amplifier::r255();
//> 1006: test::amplifier::r255();
//> 1007: test::amplifier::r255();
//> 1008: test::amplifier::r255();
//> 1009: test::amplifier::r255();
//> 1010: test::amplifier::r255();
//> 1011: test::amplifier::r255();
//> 1012: test::amplifier::r255();
//> 1013: test::amplifier::r255();
//> 1014: test::amplifier::r255();
//> 1015: test::amplifier::r255();
//> 1016: test::amplifier::r255();
//> 1017: test::amplifier::r255();
//> 1018: test::amplifier::r255();
//> 1019: test::amplifier::r255();
//> 1020: test::amplifier::r255();
//> 1021: test::amplifier::r255();
//> 1022: test::amplifier::r255();
