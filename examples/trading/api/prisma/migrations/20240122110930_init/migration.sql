-- CreateTable
CREATE TABLE "Locked" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "objectId" TEXT NOT NULL,
    "keyId" TEXT,
    "creator" TEXT,
    "deleted" BOOLEAN NOT NULL DEFAULT false
);

-- CreateTable
CREATE TABLE "Escrow" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "objectId" TEXT NOT NULL,
    "sender" TEXT,
    "recipient" TEXT,
    "keyId" TEXT,
    "swapped" BOOLEAN NOT NULL DEFAULT false,
    "cancelled" BOOLEAN NOT NULL DEFAULT false
);

-- CreateTable
CREATE TABLE "Cursor" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "eventSeq" TEXT NOT NULL,
    "txDigest" TEXT NOT NULL
);

-- CreateIndex
CREATE UNIQUE INDEX "Locked_objectId_key" ON "Locked"("objectId");

-- CreateIndex
CREATE INDEX "Locked_deleted_idx" ON "Locked"("deleted");

-- CreateIndex
CREATE UNIQUE INDEX "Escrow_objectId_key" ON "Escrow"("objectId");

-- CreateIndex
CREATE INDEX "Escrow_cancelled_idx" ON "Escrow"("cancelled");

-- CreateIndex
CREATE INDEX "Escrow_recipient_idx" ON "Escrow"("recipient");

-- CreateIndex
CREATE INDEX "Escrow_swapped_idx" ON "Escrow"("swapped");

-- CreateIndex
CREATE INDEX "Escrow_sender_idx" ON "Escrow"("sender");
