import '../styles/style.scss'
import Footer from '../components/Footer'
import Head from 'next/head'
import Main from '../components/Main'

function MyApp() {
  return (
    <>
      <Head>
        <title>Satoshi Multiplayer</title>
        <meta name="description" content="Satoshi Multiplayer game" />
      </Head>
      
      <main>
        <Main />
      </main>

      <footer className="bg-black">
        <Footer />
      </footer>
    </>
  )
}

export default MyApp
