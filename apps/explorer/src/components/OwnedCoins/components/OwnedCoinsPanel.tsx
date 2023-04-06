import { useState, useRef, useEffect } from "react"
import CoinItem from "./CoinItem"

const CoinsPanel = ({ coins, fetchCoins, nextCursor, hasNextPage }: 
    { hasNextPage: boolean, nextCursor?: string | null, coins: any[], fetchCoins: (nextCursor: string) => void 
    }) => {
    const [isVisible, setIsVisible] = useState(false)
    const containerRef = useRef(null)
    const options = {
        root: null,
        rootMargin: "0px",
        threshold: 0.1
    }

    useEffect(() => {
        const observer = new IntersectionObserver((entries) => {
            const entry = entries.pop()
            entry && setIsVisible(entry.isIntersecting)
        }, options)

        if (containerRef.current) observer.observe(containerRef.current)

        return () => {
            if (containerRef.current) observer.unobserve(containerRef.current)
        }
    }, [containerRef, options])

    useEffect(() => {
        if(isVisible && hasNextPage && nextCursor) {
            console.log('fetching here')
            fetchCoins(nextCursor)
        }
    }, [isVisible])

    return <div id="coinspanel">
        {coins.map((obj, index) => {
            if (index === coins.length - 1) {
                console.log(obj.coinType)
                return <div ref={containerRef} id="lastcoin">
                    <CoinItem key={obj.coinObjectId} coin={obj} />
                </div>
            }
            return <CoinItem key={obj.coinObjectId} coin={obj} />
        })}
    </div>
}
export default CoinsPanel