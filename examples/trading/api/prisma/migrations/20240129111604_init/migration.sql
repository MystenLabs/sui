-- DropIndex
DROP INDEX "Escrow_swapped_idx";

-- DropIndex
DROP INDEX "Escrow_cancelled_idx";

-- AlterTable
ALTER TABLE "Escrow" ADD COLUMN "itemId" TEXT;

-- AlterTable
ALTER TABLE "Locked" ADD COLUMN "itemId" TEXT;

-- CreateIndex
CREATE INDEX "Locked_creator_idx" ON "Locked"("creator");
