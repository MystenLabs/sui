import { styled } from "./stitches";

const Container = styled("div", {
  display: "flex",
  flexDirection: "column",
  gap: "$5",
});

const Heading = styled("h3", {
  color: "$textDark",
  fontSize: "$sm",
  margin: 0,
  marginBottom: "$1",
});

const Description = styled("div", {
  color: "$textLight",
  fontSize: "$sm",
  fontWeight: "$copy",
  lineHeight: "1.3",
});

export function WhatIsAWallet() {
  return (
    <Container>
      <div>
        <Heading>Easy Login</Heading>
        <Description>
          No need to create new accounts and passwords for every website. Just
          connect your wallet and get going.
        </Description>
      </div>

      <div>
        <Heading>Store your Digital Assets</Heading>
        <Description>
          Send, receive, store, and display your digital assets like NFTs &
          coins.
        </Description>
      </div>
    </Container>
  );
}
