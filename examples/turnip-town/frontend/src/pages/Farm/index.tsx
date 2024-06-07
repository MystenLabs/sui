import { useEffect } from "react";

const CROPS_FENCE_TOP = 740;
const CROPS_FENCE_LEFT = 180;
const CROP_WIDTH = 95;
const CROP_HEIGHT = 52.5;
const CANVAS_WIDTH = 760;
const CANVAS_HEIGHT = 1280;
const TOTAL_ROWS = 4;
const TOTAL_COLS = 4;

const CHARACTER_SPRITES = new Image();
CHARACTER_SPRITES.src = `/assets/character.png`;

const CROP_SPRITES = new Image();
CROP_SPRITES.src = `/assets/sprites.png`;

const OBSTACLES = [
    // Water bucket
    { x: 37, y: 573, w: 74, h: 84 },
    // Fence left side
    { x: 157, y: 691, w: 25, h: 310 },
    // fence right side
    { x: 560, y: 681, w: 25, h: 320 },
    // fence top right
    { x: 489, y: 729, w: 95, h: 11 },
    // fence top left
    { x: 161, y: 729, w: 100, h: 11 },
    // fence bottom
    { x: 170, y: 998, w: 400, h: 12 }
]

const hasCollision = (x: number, y: number, characterWidth: number, characterHeight: number) => {
    for (const obstacle of OBSTACLES) {

        const obstacleX = obstacle.x - (characterWidth / 2);
        // const obs

        console.log({ x, y });
        if (x >= obstacleX && x <= obstacleX + obstacle.w 
                && y >= obstacle.y && y <= obstacle.y + obstacle.h) {
                    return true;
            }
    }
    return false;
}

enum Moves {
    UP = 'Up',
    DOWN = 'Down',
    LEFT = 'Left',
    RIGHT = 'Right'
}

const DEFAULT_MOVES: Record<Moves, number> = {
    [Moves.UP]: 0,
    [Moves.DOWN]: 0,
    [Moves.LEFT]: 0,
    [Moves.RIGHT]: 0
}

class Character {
    timeout?: any;
    x: number;
    y: number;
    height: number;
    width: number;
    activeMoves: Record<Moves, number>;
    previousMoves: Record<Moves, number>;
    movementSpeed: number = 15;
    ctx: CanvasRenderingContext2D | null | undefined;

    constructor(x: number, y: number, width: number, height: number, ctx?: CanvasRenderingContext2D) {
        this.x = x;
        this.y = y;
        this.width = width;
        this.height = height;
        this.previousMoves = DEFAULT_MOVES;
        this.activeMoves = DEFAULT_MOVES;
        if (ctx) this.ctx = ctx;
    }
    
    update() {
        let x = this.x;
        let y = this.y;
    
        if (this.activeMoves[Moves.UP] && this.activeMoves[Moves.UP] > this.previousMoves[Moves.UP]) {
            y = y - this.movementSpeed;
        }

        if (this.activeMoves[Moves.DOWN] && this.activeMoves[Moves.DOWN] > this.previousMoves[Moves.DOWN]) {
            y = y + this.movementSpeed;
        }

        if (this.activeMoves[Moves.LEFT] && this.activeMoves[Moves.LEFT] > this.previousMoves[Moves.LEFT]) {
            x = x - this.movementSpeed;
        }

        if (this.activeMoves[Moves.RIGHT] && this.activeMoves[Moves.RIGHT] > this.previousMoves[Moves.RIGHT]) {
            x = x + this.movementSpeed;
        }

        if (hasCollision(x, y)) {
            return;
        }

        if (x !== this.x) this.x = x;
        if (y !== this.y) this.y = y;

        // handle out of bounds
        if (x < 0) this.x = 0;
        if (y < 0) this.y = 0;

        const maxX = CANVAS_WIDTH - this.width;
        const maxY = CANVAS_HEIGHT - this.height;

        
        if (x > maxX) this.x = maxX;
        if (y > maxY) this.y = maxY;

        const inCropsOnXAxis = (x + this.width/2) > CROPS_FENCE_LEFT && x < CROPS_FENCE_LEFT + (CROP_WIDTH * TOTAL_COLS);
        const inCropsOnYAxis = (y + this.height / 2) > CROPS_FENCE_TOP && y < CROPS_FENCE_TOP + (CROP_HEIGHT * TOTAL_ROWS);

        if (inCropsOnXAxis && inCropsOnYAxis) {
            // render the active tile
            const activeCol = Math.floor((x + (this.width/2) - CROPS_FENCE_LEFT) / CROP_WIDTH);
            const activeRow = Math.floor((y + (this.height/2) - CROPS_FENCE_TOP) / CROP_HEIGHT);

            if (activeCol >= TOTAL_COLS || activeRow >= TOTAL_ROWS) return;

            character.ctx!.strokeStyle = "#417777"
            character.ctx!.lineWidth = 4;
            character.ctx!.strokeRect(CROPS_FENCE_LEFT + (CROP_WIDTH * activeCol), CROPS_FENCE_TOP + (CROP_HEIGHT * activeRow), CROP_WIDTH, CROP_HEIGHT);
        }
    }

    draw() {
        if (!this.ctx) return;
        this.update();
        let yPosition: number = 3 * 112;
        if (this.activeMoves[Moves.UP]) yPosition = 5 * 112;
        if (this.activeMoves[Moves.DOWN]) yPosition = 2 * 112;
        if (this.activeMoves[Moves.LEFT]) yPosition = 0 * 112;
        if (this.activeMoves[Moves.RIGHT]) yPosition = 1 * 112;

        const xPosition = (Object.values(this.activeMoves).find(x => x > 0)! % 4) * 88 || 0;

        character.ctx!.drawImage(CHARACTER_SPRITES, xPosition, yPosition, 88, 112, this.x, this.y, this.width, this.height);
    }

    clear() {
        if (!this.ctx) return;
        this.ctx.clearRect(this.x, this.y, this.width, this.height)
    }

    setActiveMoves(moves: Record<Moves, number>) {
        this.previousMoves = structuredClone(this.activeMoves);
        this.activeMoves = moves;
    }

    setCtx(ctx: CanvasRenderingContext2D) {
        this.ctx = ctx;
    }

    isCtxSet() {
        return !!this.ctx;
    }

    resetMoves() {
        this.activeMoves = structuredClone(DEFAULT_MOVES);
    }

    updateTimeout(timeout: any) {
        this.timeout = timeout;
    }
}


const character = new Character(400, 800, 88, 112);

const handleMovementEvent = (event: KeyboardEvent) => {
    let move: Moves;
    if (event.code === 'KeyW') move = Moves.UP;
    if (event.code === 'KeyS') move = Moves.DOWN;
    if (event.code === 'KeyA') move = Moves.LEFT;
    if (event.code === 'KeyD') move = Moves.RIGHT;

    // @ts-ignore-next-line
    if (!move) return;

    const nextMoves = structuredClone(DEFAULT_MOVES);
    nextMoves[move] = character.activeMoves[move] + 1;

    if (character.timeout){
        clearTimeout(character.timeout);
        character.timeout = null;
    }

    character.setActiveMoves(nextMoves);

    if (!character.timeout) {
        character.timeout = setTimeout(() => {
            // character.resetMoves();
            character.timeout = null;
        }, 500);
    };
    window.requestAnimationFrame(updateFrames);
}

const updateFrames = () => {
    let ctx = character.ctx;
    if (!ctx) {
        const canvas = document.getElementById("turnip-town-farm") as HTMLCanvasElement;
        ctx = canvas.getContext("2d");
        if (!ctx) return;
        character.setCtx(ctx);
    }
    ctx.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT);
    for(let i = 0; i < TOTAL_ROWS; i++) {
        for (let j=0; j < TOTAL_COLS; j++ ) {
            drawCrop(ctx, CROPS_FENCE_LEFT + (CROP_WIDTH * i), CROPS_FENCE_TOP + (CROP_HEIGHT * j), 11 - j - i);
        }
    }

    character.draw();
    // window.requestAnimationFrame(updateFrames);
}

const drawCrop = (ctx: CanvasRenderingContext2D, x: number, y: number, state: number) => {
    const pos = {
        spriteX: (48 * state),
        spriteY: 0,
        width: 48,
        height: 48,
        x: x + 20,
        y: y - 17,
        finalWidth: 48 * 1.6,
        finalHeight: 48 * 1.6
    }
    
    ctx.drawImage(CROP_SPRITES, pos.spriteX, pos.spriteY, pos.width, pos.height, pos.x, pos.y, pos.finalWidth, pos.finalHeight);
}

let i = 0;
export function Farm () {

    useEffect(() => {
        document.addEventListener('keydown', handleMovementEvent);
        document.addEventListener('keyup', handleMovementEvent);
        window.requestAnimationFrame(updateFrames);

        // setInterval(() => {
        //     character.setActiveMoves({
        //         ...DEFAULT_MOVES, 
        //         [Moves.DOWN]: i++
        //     });
        //     window.requestAnimationFrame(updateFrames);
        // }, 100)
        return () => {
            document.removeEventListener('keydown', handleMovementEvent);
            document.removeEventListener('keyup', handleMovementEvent);
        }
    }, []);

    return (
        <div>
            <canvas id="turnip-town-farm" height="1280" width="760" style={{margin: "0 auto", backgroundImage: "url('/assets/farm-background.jpg", backgroundSize: "cover"}}>
            </canvas>
        </div>
    )
}
