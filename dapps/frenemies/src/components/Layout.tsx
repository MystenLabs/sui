// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Header from "./header/Header";
import Footer from "./footer/Footer";
import Leaderboard from "./leaderboard/Leaderboard";

function Layout() {
  return (
    <div className="container">
      <Header />

      <div className="mx-auto max-w-4xl container">
        {/* The data should later be fetched from Sui Network directly */}
        <Leaderboard
          rank={333}
          totalScore={420}
          records={[
            {
              round: 10,
              role: "enemy",
              validator: "0x0000000000000000000000000000000000000000",
              objectiveAchieved: true,
              score: 100,
            },
          ]}
        />
      </div>

      <Footer />
    </div>
  );
}

export default Layout;
