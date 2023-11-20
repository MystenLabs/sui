---
title: SUI Weather Oracle
---

This guide demonstrates writing a module (smart contract) in Move, deploying it on Devnet, and adding a back-end, which fetches the weather data from the OpenWeather API every 10 minutes and updates the weather conditions for each city. SUI Weather Oracle is a dApp that provides real-time weather data for over 1,000 locations around the world. 

The data is sourced from the OpenWeather API. The user can access and use the weather data for various applications, such as randomness, betting, gaming, insurance, travel, education, or research. The user can also mint a weather NFT based on the weather data of a city, using the `mint` function of the SUI Weather Oracle smart contract.

This guide assumes you have [installed Sui](../getting-started/sui-install.mdx) and understand Sui fundamentals.

## SUI Move Smart Contract

As with all SUI dApps, a Move package on chain powers the logic of SUI Weather Oracle. The following instruction walks you through creating and publishing the module.

### Weather Oracle module

Before you get started, you must initialize a Move package. Open a terminal or console in the directory you want to store the example and run the following command to create an empty package with the name `weather_oracle`:

```bash
sui move new weather_oracle
```

With that done, it's time to jump into some code. Create a new file in the `sources` directory with the name `weather.move` and populate the file with the following code:

```rust title='weather.move'
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module oracle::weather {
    use std::option::{Self, Option};
    use std::string::{Self, String};
    use sui::dynamic_object_field as dof;
    use sui::object::{Self, UID};
    use sui::package;
    use sui::transfer::{Self};
    use sui::tx_context::{Self, TxContext};
}
```

There are few details to take note of in this code:

1. The forth line declares the module name as `weather` within the package `oracle`.
1. Seven lines begin with the `use` keyword, which enables this module to use types and functions declared in other modules.

Next, add some more code to this module:

```rust title='weather.move'
    /// Define a capability for the admin of the oracle.
    struct AdminCap has key, store { id: UID }

    /// // Define a one-time witness to create the `Publisher` of the oracle.
    struct WEATHER has drop {}

    // Define a struct for the weather oracle
    struct WeatherOracle has key {
        id: UID,
        /// The address of the oracle.
        address: address,
        /// The name of the oracle.
        name: String,
        /// The description of the oracle.
        description: String,
    }

    struct CityWeatherOracle has key, store {
        id: UID,
        geoname_id: u32, // The unique identifier of the city
        name: String, // The name of the city
        country: String, // The country of the city
        latitude: u32, // The latitude of the city in degrees
        positive_latitude: bool, // Whether the latitude is positive (north) or negative (south)
        longitude: u32, // The longitude of the city in degrees
        positive_longitude: bool, // Whether the longitude is positive (east) or negative (west)
        weather_id: u16, // The weather condition code
        temp: u32, // The temperature in kelvin
        pressure: u32, // The atmospheric pressure in hPa
        humidity: u8, // The humidity percentage
        visibility: u16, // The visibility in meters
        wind_speed: u16, // The wind speed in meters per second
        wind_deg: u16, // The wind direction in degrees
        wind_gust: Option<u16>, // The wind gust in meters per second (optional)
        clouds: u8, // The cloudiness percentage
        dt: u32 // The timestamp of the weather update in seconds since epoch
    }

    fun init(otw: WEATHER, ctx: &mut TxContext) {
        package::claim_and_keep(otw, ctx); // Claim ownership of the one-time witness and keep it

        let cap = AdminCap { id: object::new(ctx) }; // Create a new admin capability object
        transfer::share_object(WeatherOracle {
            id: object::new(ctx),
            address: tx_context::sender(ctx),
            name: string::utf8(b"SuiMeteo"),
            description: string::utf8(b"A weather oracle."),
        });
        transfer::public_transfer(cap, tx_context::sender(ctx)); // Transfer the admin capability to the sender.
    }
```

- The first struct, `AdminCap`, is a [capability](concepts/sui-move-concepts/patterns/capabilities.mdx) that initializes the house data.
- The second struct, `WEATHER`, is a [one-time witness](concepts/sui-move-concepts/one-time-witness.mdx) that ensures only a single instance of this `Weather` ever exists.
- The `WeatherOracle` struct works as registry and stores the `geoname_id`s of the `CityWeatherOracle`s as [Dynamic Fields](concepts/dynamic-fields/dynamic-object-fields.mdx).
- The [`init` function](concepts/sui-move-concepts/init.mdx) creates and sends the `Publisher` and `AdminCap` objects to the sender. Also it creates a [shared object](concepts/object-ownership/shared.mdx) for all the `CityWeatherOracle`s.

So far, you've set up the data structures within the module.
Now, create a function that initializes a `CityWeatherOracle` and adds it as [Dynamic Fields](concepts/dynamic-fields/dynamic-object-fields.mdx)  to the `WeatherOracle` object:

```rust title='weather.move'
    public fun add_city(
        _: &AdminCap, // The admin capability
        oracle: &mut WeatherOracle, // A mutable reference to the oracle object
        geoname_id: u32, // The unique identifier of the city
        name: String, // The name of the city
        country: String, // The country of the city
        latitude: u32, // The latitude of the city in degrees
        positive_latitude: bool, // The whether the latitude is positive (north) or negative (south)
        longitude: u32, // The longitude of the city in degrees
        positive_longitude: bool, // The whether the longitude is positive (east) or negative (west)
        ctx: &mut TxContext // A mutable reference to the transaction context
    ) {
        dof::add(&mut oracle.id, geoname_id, // Add a new dynamic object field to the oracle object with the geoname ID as the key and a new city weather oracle object as the value.
            CityWeatherOracle {
                id: object::new(ctx), // Assign a unique ID to the city weather oracle object 
                geoname_id, // Set the geoname ID of the city weather oracle object
                name,  // Set the name of the city weather oracle object
                country,  // Set the country of the city weather oracle object
                latitude,  // Set the latitude of the city weather oracle object
                positive_latitude,  // Set whether the latitude is positive (north) or negative (south)
                longitude,  // Set the longitude of the city weather oracle object
                positive_longitude,  // Set whether the longitude is positive (east) or negative (west)
                weather_id: 0, // Initialize the weather condition code to be zero 
                temp: 0, // Initialize the temperature to be zero 
                pressure: 0, // Initialize the pressure to be zero 
                humidity: 0, // Initialize the humidity to be zero 
                visibility: 0, // Initialize the visibility to be zero 
                wind_speed: 0, // Initialize the wind speed to be zero 
                wind_deg: 0, // Initialize the wind direction to be zero 
                wind_gust: option::none(), // Initialize the wind gust to be none 
                clouds: 0, // Initialize the cloudiness to be zero 
                dt: 0 // Initialize the timestamp to be zero 
            }
        );
    }
```

The `add_city` function is a public function that allows the owner of the `AdminCap` of the SUI Weather Oracle smart contract to add a new `CityWeatherOracle`. The function requires the admin to provide a capability object that proves their permission to add a city. The function also requires a mutable reference to the oracle object, which is the main object that stores the weather data on the blockchain. The function takes several parameters that describe the city, such as the geoname ID, name, country, latitude, longitude, and positive latitude and longitude. The function then creates a new city weather oracle object, which is a sub-object that stores and updates the weather data for a specific city. The function initializes the city weather oracle object with the parameters provided by the admin, and sets the weather data to be zero or none. The function then adds a new dynamic object field to the oracle object, using the geoname ID as the key and the city weather oracle object as the value. This way, the function adds a new city to the oracle, and makes it ready to receive and update the weather data from the back-end service.

If you want to delete a city from the SUI Weather Oracle, you need to call the `remove_city` function of the smart contract. The `remove_city` function allows the admin of the smart contract to remove a city from the oracle. The function requires the admin to provide a capability object that proves their permission to remove a city. The function also requires a mutable reference to the oracle object, which is the main object that stores and updates the weather data on the blockchain. The function takes the geoname ID of the city as a parameter, and deletes the city weather oracle object for the city. The function also removes the dynamic object field for the city from the oracle object. This way, the function deletes a city from the oracle, and frees up some storage space on the blockchain.

```rust title='weather.move'
public fun remove_city(
    _: &AdminCap,
    oracle: &mut WeatherOracle, 
    geoname_id: u32
    ) {
        let CityWeatherOracle { 
            id, 
            geoname_id: _, 
            name: _, 
            country: _, 
            latitude: _, 
            positive_latitude: _, 
            longitude: _, 
            positive_longitude: _, 
            weather_id: _, 
            temp: _, 
            pressure: _, 
            humidity: _, 
            visibility: _, 
            wind_speed: _, 
            wind_deg: _, 
            wind_gust: _, 
            clouds: _, 
            dt: _ } = dof::remove(&mut oracle.id, geoname_id);
        object::delete(id);
}
```

Now that you have implemented the `add_city` and `remove_city` functions, you can move on to the next step, which is to see how you can update the weather data for each city. The weather data is fetched from the OpenWeather API every 10 minutes by the back-end service, and then passed to the `update` function of the SUI Weather Oracle smart contract. The `update` function takes the geoname ID and the new weather data of the city as parameters, and updates the city weather oracle object with the new data. This way, the weather data on the blockchain is always up to date and accurate.

```rust title='weather.move'
    public fun update(
        _: &AdminCap,
        oracle: &mut WeatherOracle,
        geoname_id: u32,
        weather_id: u16,
        temp: u32,
        pressure: u32,
        humidity: u8,
        visibility: u16,
        wind_speed: u16,
        wind_deg: u16,
        wind_gust: Option<u16>,
        clouds: u8,
        dt: u32
    ) {
        let city_weather_oracle_mut = dof::borrow_mut<u32, CityWeatherOracle>(&mut oracle.id, geoname_id); // Borrow a mutable reference to the city weather oracle object with the geoname ID as the key
        city_weather_oracle_mut.weather_id = weather_id;
        city_weather_oracle_mut.temp = temp;
        city_weather_oracle_mut.pressure = pressure;
        city_weather_oracle_mut.humidity = humidity;
        city_weather_oracle_mut.visibility = visibility;
        city_weather_oracle_mut.wind_speed = wind_speed;
        city_weather_oracle_mut.wind_deg = wind_deg;
        city_weather_oracle_mut.wind_gust = wind_gust;
        city_weather_oracle_mut.clouds = clouds;
        city_weather_oracle_mut.dt = dt;
    }
```

You have defined the data structure of the SUI Weather Oracle smart contract, but you need some functions to access and manipulate the data. Now you will add some helper functions that read and return the weather data for a `WeatherOracle` object. These functions will allow you to get the weather data for a specific city in the oracle. These functions will also allow you to format and display the weather data in a user-friendly way.

```rust title='weather.move'
    // --------------- Read-only References ---------------

    /// Returns the `name` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_name(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): String {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.name
    }
    /// Returns the `country` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_country(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): String {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.country
    }
    /// Returns the `latitude` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_latitude(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u32 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.latitude
    }
    /// Returns the `positive_latitude` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_positive_latitude(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): bool {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.positive_latitude
    }
    /// Returns the `longitude` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_longitude(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): u32 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.longitude
    }
    /// Returns the `positive_longitude` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_positive_longitude(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): bool {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.positive_longitude
    }
    /// Returns the `weather_id` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_weather_id(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u16 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.weather_id
    }
    /// Returns the `temp` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_temp(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u32 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.temp
    }
    /// Returns the `pressure` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_pressure(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): u32 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.pressure
    }
    /// Returns the `humidity` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_humidity(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): u8 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.humidity
    }
    /// Returns the `visibility` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_visibility(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u16 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.visibility
    }
    /// Returns the `wind_speed` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_wind_speed(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u16 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.wind_speed
    }
    /// Returns the `wind_deg` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_wind_deg(
        weather_oracle: &WeatherOracle, 
        geoname_id: u32
    ): u16 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.wind_deg
    }
    /// Returns the `wind_gust` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_wind_gust(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): Option<u16> {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.wind_gust
    }
    /// Returns the `clouds` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_clouds(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u8 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.clouds
    }
    /// Returns the `dt` of the `CityWeatherOracle` with the given `geoname_id`.
    public fun city_weather_oracle_dt(
        weather_oracle: &WeatherOracle,
        geoname_id: u32
    ): u32 {
        let city_weather_oracle = dof::borrow<u32, CityWeatherOracle>(&weather_oracle.id, geoname_id);
        city_weather_oracle.dt
    }
```

To conclude this guide, we will show you how to add an extra feature that will allow anyone to mint a `WeatherNFT` with the current conditions of a city by passing the geoname ID. The `mint` function is a public function that allows anyone to mint a weather NFT based on the weather data of a city. The function takes the `WeatherOracle` shared object and the geoname ID of the city as parameters, and returns a new `WeatherNFT` object for the city. The `WeatherNFT` object is a unique and non-fungible token that represents the weather of the city at the time of minting. The `WeatherNFT` object has the same data as the `CityWeatherOracle` object, such as the `geonameID`, `name`, `country`, `latitude`, `longitude`, `positive latitude` and `longitude`, `weather ID`, `temperature`, `pressure`, `humidity`, `visibility`, `wind speed`, `wind degree`, `wind gust`, `clouds`, and `timestamp`. The function creates the `WeatherNFT` object by borrowing a reference to the `CityWeatherOracle` object with the geoname ID as the key, and assigning a unique ID (`UID`) to the `WeatherNFT` object. The function then transfers the ownership of the `WeatherNFT` object to the sender of the transaction. This way, the function allows anyone to mint a weather NFT and own a digital representation of the weather of a city. You can use this feature to create your own collection of weather NFTs, or to use them for other applications that require verifiable and immutable weather data.

And with that, your `weather.move` code is complete. We hope you enjoyed this guide and learned how to create a simple weather oracle on the SUI blockchain.

## Deployment

:::info 

See [Publish a Package](guides/developer/first-app/publish) for a more detailed guide on publishing packages or [Sui Client CLI](/references/cli/client.mdx) for a complete reference of `client` commands in the Sui CLI.

:::

Before [publishing your code](guides/developer/first-app/publish), you must first initialize the Sui Client CLI. To do so, in a terminal or console at the root directory of the project enter `sui client`. You will then see:

```
Config file ["[LINK_TO_PATH/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui Full node server [y/N]?
```

Enter `y` to proceed. You then see:

```
Sui Full node server URL (Defaults to Sui Devnet if not specified) :
```

Leave this blank (press Enter). Then you see:

```
Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1, 2: for secp256r1):
```

Select `0`. Now you should have a Sui address set up.

Before being able to publish your package to Devnet, however, you need Devnet SUI tokens. To get some, join the [Sui Discord](https://discord.gg/Sui), complete the verification steps, enter the `#devnet-faucet` channel and type `!faucet <WALLET ADDRESS>`. For other ways to get SUI in your Devnet account, see [Get SUI Tokens](/guides/developer/getting-started/get-coins).

Now that you have an account with some Devnet SUI, you can deploy your contracts. To publish your package, use the following command in the same terminal or console:

```
sui client publish --gas-budget <GAS-BUDGET>
```

For the gas budget, use a standard value such as `20000000`.

You have successfully deployed the SUI Weather Oracle smart contract on the blockchain. Now, it's time to create an Express back-end that can interact with it. The Express back-end will perform the following tasks:

- It will initialize the smart contract with 1,000 cities using the `add_city` function of the smart contract. The back-end will pass the geoname ID, name, country, latitude, longitude, and positive latitude and longitude of each city as parameters to the function.
- It will fetch the weather data for each city from the OpenWeather API every 10 minutes, using the API key that you obtained from the website. The back-end will parse the JSON response and extract the weather data for each city, such as the weather ID, temperature, pressure, humidity, visibility, wind speed, wind degree, wind gust, clouds, and timestamp.
- It will update the weather data for each city on the blockchain, using the `update` function of the smart contract. The back-end will pass the geoname ID and the new weather data of each city as parameters to the function.

The Express back-end will use the SUI SDK, which is a TypeScript library that allows you to interact with the SUI blockchain and smart contracts. You can install the SUI SDK from [this](https://sui-typescript-docs.vercel.app/typescript). You will use the SUI SDK to connect to the SUI network, sign and submit transactions, and query the state of the smart contract. You will also use the SUI SDK to mint weather NFTs, if you want to use that feature of the smart contract.

## Backend

In this section, you create a Express back-end project using the [Sui Typescript SDK](https://sui-typescript-docs.vercel.app/typescript) and the [Sui dApp Kit](https://sui-typescript-docs.vercel.app/dapp-kit) that interacts with the deployed smart contracts.

In this section, you will learn how to create an Express back-end project that interacts with the SUI Weather Oracle smart contract that you deployed on the blockchain. You will use the (SUI Typescript SDK)[(https://sui-typescript-docs.vercel.app/typescript)] to connect to the SUI network, sign and submit transactions, and query the state of the smart contract. You will also use the OpenWeather API to fetch the weather data for each city and update the smart contract every 10 minutes. You will also be able to mint weather NFTs using the `mint` function of the smart contract.

### Initialize the project

First, initialize your back-emd project. To do this, you need to follow these steps:
- Create a new folder named `weather-oracle-backend` and navigate to it in your terminal.
- Run `npm init -y` to create a package.json file with default values.
- Run `npm install express --save` to install express as a dependency and save it to your package.json file.
- Run `npm install @mysten/bcs @mysten/sui.js axios csv-parse csv-parser dotenv pino retry-axios --save` to install the other dependencies and save them to your package.json file. These dependencies are:
    - **@mysten/bcs**: a library for blockchain services.
    - **@mysten/sui.js**: a library for smart user interfaces.
    - **axios**: a library for making HTTP requests.
    - **csv-parse**: a library for parsing CSV data.
    - **csv-parser**: a library for transforming CSV data into JSON objects.
    - **dotenv**: a library for loading environment variables from a .env file.
    - **pino**: a library for fast and low-overhead logging.
    - **retry-axios**: a library for retrying failed axios requests.
- Create a new file named `init.ts` and write your express code in it.

//////
The code is a script that uses the @mysten/sui.js library to interact with a blockchain-based weather oracle. The script does the following:
- It imports the necessary modules and classes from the library, such as Connection, Ed25519Keypair, JsonRpcProvider, RawSigner, and TransactionBlock.
- It imports the dotenv module to load environment variables from a .env file.
- It imports some custom modules and functions from the local files, such as City, get1000Geonameids, getCities, getWeatherOracleDynamicFields, and logger.
- It derives a keypair from a phrase stored in the ADMIN_PHRASE environment variable.
- It creates a provider object that connects to a fullnode specified by the FULLNODE environment variable.
- It creates a signer object that uses the keypair and the provider to sign and execute transactions on the blockchain.
- It reads some other environment variables, such as PACKAGE_ID, ADMIN_CAP_ID, WEATHER_ORACLE_ID, and MODULE_NAME, which are used to identify the weather oracle contract and its methods.
- It defines a constant NUMBER_OF_CITIES, which is the number of cities to be added to the weather oracle in each batch.
- It defines an async function addCityWeather, which does the following:
    - It gets an array of cities from the getCities function.
    - It gets an array of 1000 geonameids from the get1000Geonameids function.
    - It gets an array of weather oracle dynamic fields from the getWeatherOracleDynamicFields function, which contains the geonameids of the existing cities in the weather oracle.
    - It initializes a counter and a transaction block object.
    - It loops through the cities array and checks if the city's geonameid is not in the weather oracle dynamic fields array and is in the 1000 geonameids array.
    - If the condition is met, it adds a moveCall to the transaction block, which calls the add_city method of the weather oracle contract with the city's information, such as geonameid, asciiname, country, latitude, and longitude.
    - It increments the counter and checks if it reaches the NUMBER_OF_CITIES. If so, it calls another async function signAndExecuteTransactionBlock with the transaction block as an argument, which signs and executes the transaction block on the blockchain and logs the result. It then resets the counter and the transaction block.
    - After the loop ends, it calls the signAndExecuteTransactionBlock function again with the remaining transaction block.
- It calls the addCityWeather function.
//////

```typescript title='init.ts'
import {
  Connection,
  Ed25519Keypair,
  JsonRpcProvider,
  RawSigner,
  TransactionBlock,
} from "@mysten/sui.js";
import * as dotenv from "dotenv";
import { City } from "./city";
import { get1000Geonameids } from "./filter-cities";
import { latitudeMultiplier, longitudeMultiplier } from "./multipliers";
import { getCities, getWeatherOracleDynamicFields } from "./utils";
import { logger } from "./utils/logger";

dotenv.config({ path: "../.env" });

const phrase = process.env.ADMIN_PHRASE;
const fullnode = process.env.FULLNODE!;
const keypair = Ed25519Keypair.deriveKeypair(phrase!);
const provider = new JsonRpcProvider(
  new Connection({
    fullnode: fullnode,
  })
);
const signer = new RawSigner(keypair, provider);

const packageId = process.env.PACKAGE_ID;
const adminCap = process.env.ADMIN_CAP_ID!;
const weatherOracleId = process.env.WEATHER_ORACLE_ID!;
const moduleName = "weather";

logger.info("packageId", packageId);
logger.info("adminCap", adminCap);
logger.info("weatherOracleId", weatherOracleId);

const NUMBER_OF_CITIES = 10;

async function addCityWeather() {
  const cities: City[] = await getCities();
  const thousandGeoNameIds = await get1000Geonameids();

  const weatherOracleDynamicFields = await getWeatherOracleDynamicFields(
    provider,
    weatherOracleId
  );
  const geonames = weatherOracleDynamicFields.map(function (obj) {
    return obj.name;
  });

  let counter = 0;
  let transactionBlock = new TransactionBlock();
  for (let c in cities) {
    logger.info(cities[c].name, cities[c].geonameid);

    if (
      !geonames.includes(cities[c].geonameid) &&
      thousandGeoNameIds.includes(cities[c].geonameid)
    ) {
      transactionBlock.moveCall({
        target: `${packageId}::${moduleName}::add_city`,
        arguments: [
          transactionBlock.object(adminCap), // adminCap
          transactionBlock.object(weatherOracleId), // WeatherOracle
          transactionBlock.pure(cities[c].geonameid), // geoname_id
          transactionBlock.pure(cities[c].asciiname), // asciiname
          transactionBlock.pure(cities[c].countryCode), // country
          transactionBlock.pure(cities[c].latitude * latitudeMultiplier), // latitude
          transactionBlock.pure(cities[c].latitude > 0), // positive_latitude
          transactionBlock.pure(cities[c].longitude * longitudeMultiplier), // longitude
          transactionBlock.pure(cities[c].longitude > 0), // positive_longitude
        ],
      });

      counter++;
      if (counter === NUMBER_OF_CITIES) {
        await signAndExecuteTransactionBlock(transactionBlock);
        counter = 0;
        transactionBlock = new TransactionBlock();
      }
    }
  }
  await signAndExecuteTransactionBlock(transactionBlock);
}

async function signAndExecuteTransactionBlock(
  transactionBlock: TransactionBlock
) {
  transactionBlock.setGasBudget(5000000000);
  await signer
    .signAndExecuteTransactionBlock({
      transactionBlock,
      requestType: "WaitForLocalExecution",
      options: {
        showObjectChanges: true,
        showEffects: true,
      },
    })
    .then(function (res) {
      logger.info(res);
    });
}

addCityWeather();
```

```typescript title='App.tsx'
import { ConnectButton, useCurrentAccount } from "@mysten/dapp-kit";
import { Box, Callout, Container, Flex, Grid, Heading } from "@radix-ui/themes";
import { PlayerSesh } from "./containers/Player/PlayerSesh";
import { HouseSesh } from "./containers/House/HouseSesh";
import { HOUSECAP_ID, PACKAGE_ID } from "./constants";
import { InfoCircledIcon } from "@radix-ui/react-icons";

function App() {
  const account = useCurrentAccount();
  return (
    <>
      <Flex
        position="sticky"
        px="4"
        py="2"
        justify="between"
        style={{
          borderBottom: "1px solid var(--gray-a2)",
        }}
      >
        <Box>
          <Heading>Satoshi Coin Flip Single Player</Heading>
        </Box>

        <Box>
          <ConnectButton />
        </Box>
      </Flex>
      <Container>
        <Heading size="4" m={"2"}>
          Package ID: {PACKAGE_ID}
        </Heading>
        <Heading size="4" m={"2"}>
          HouseCap ID: {HOUSECAP_ID}
        </Heading>

        <Callout.Root mb="2">
          <Callout.Icon>
            <InfoCircledIcon />
          </Callout.Icon>
          <Callout.Text>
            You need to connect to wallet that publish the smart contract
            package
          </Callout.Text>
        </Callout.Root>

        {!account ? (
          <Heading size="4" align="center">
            Please connect wallet to continue
          </Heading>
        ) : (
          <Grid columns="2" gap={"3"} width={"auto"}>
            <PlayerSesh />
            <HouseSesh />
          </Grid>
        )}
      </Container>
    </>
  );
}

export default App;
```

Like other dApps, you need a "connect wallet" button to enable connecting users' wallets. dApp Kit contains a pre-made `ConnectButton` React component that you can reuse to help users onboard.

`useCurrentAccount()` is a React hook the dApp Kit also provides to query the current connected wallet; returning `null` if there isn't a wallet connection. Leverage this behavior to prevent a user from proceeding further if they haven’t connected their wallet yet.

There are two constants that you need to put into `constants.ts` to make the app work – `PACKAGE_ID` and `HOUSECAP_ID`. You can get these from the terminal or console after running the Sui CLI command to publish the package.

After ensuring that the user has connected their wallet, you can display the two columns described in the previous section: `PlayerSesh` and `HouseSesh` components.

Okay, that’s a good start to have an overview of the project. Time to move to initializing the `HouseData` object. All the frontend logic for calling this lives in the `HouseInitialize.tsx` component. The component includes UI code, but the logic that executes the transaction follows:

```typescript title='containers/House/HouseInitialize.tsx'
<form
  onSubmit={(e) => {
    e.preventDefault();

    // Create new transaction block
    const txb = new TransactionBlock();
    // Split gas coin into house stake coin
    // SDK will take care for us abstracting away of up-front coin selections
    const [houseStakeCoin] = txb.splitCoins(txb.gas, [
      MIST_PER_SUI * BigInt(houseStake),
    ]);
    // Calling smart contract function
    txb.moveCall({
      target: `${PACKAGE_ID}::house_data::initialize_house_data`,
      arguments: [
        txb.object(HOUSECAP_ID),
        houseStakeCoin,
        // This argument is not an on-chain object, hence, we must serialize it using `bcs`
        // https://sui-typescript-docs.vercel.app/typescript/transaction-building/basics#pure-values
        txb.pure(
          bcs
            .vector(bcs.U8)
            .serialize(curveUtils.hexToBytes(getHousePubHex())),
        ),
      ],
    });

    execInitializeHouse(
      {
        transactionBlock: txb,
        options: {
          showObjectChanges: true,
        },
      },
      {
        onError: (err) => {
          toast.error(err.message);
        },
        onSuccess: (result: SuiTransactionBlockResponse) => {
          let houseDataObjId;


          result.objectChanges?.some((objCh) => {
            if (
              objCh.type === "created" &&
              objCh.objectType === `${PACKAGE_ID}::house_data::HouseData`
            ) {
              houseDataObjId = objCh.objectId;
              return true;
            }
          });

          setHouseDataId(houseDataObjId!);

          toast.success(`Digest: ${result.digest}`);
        },
      },
    );
  }}
```

To use a [programmable transaction block](/concepts/transactions/prog-txn-blocks.mdx) (PTB) in Sui, create a `TransactionBlock`. To initiate a Move call, you must know the global identifier of a public function in your smart contract. The global identifier usually takes the following form:

```
${PACKAGE_ID}::${MODULE_NAME}::${FUNCTION_NAME}
```

In this example, it is:

```
${PACKAGE_ID}::house_data::initialize_house_data
```

There are a few parameters that you need to pass into `initialize_house_data()` Move function: the `HouseCap` ID, the House stake, and the House BLS public key:

- Import the `HouseCap` ID from `constants.ts`, which you set up in the previous section.
- Use `TransactionBlock::splitCoin` for the House stake to create a new coin with a defined amount split from the Gas Coin `txb.gas`. Think of the gas coin as one singular coin available for gas payment from your account (which might cover the entire remaining balance of your account). This is useful for Sui payments - instead of manually selecting the coins for gas payment or manually splitting/merging to have the coin with correct amount for your Move call, the gas coin is the single entry point for this, with all the heavy lifting delegated to the SDK behind the scenes.
- Pass the BLS public key as bytes `vector<u8>`. When providing inputs that are not on-chain objects, serialize them as BCS using a combination of `txb.pure` and `bcs` imported from `@mysten/sui.js/bcs`.

Now sign and execute the transaction block. dApp Kit provides a React hook `useSignAndExecuteTransactionBlock()` to streamline this process. This hook, when executed, prompts the UI for you to approve, sign, and execute the transaction block. You can configure the hook with the `showObjectChanges` option to return the newly-created `HouseData` shared object as the result of the transaction block. This `HouseData` object is important as you use it as input for later Move calls, so save its ID somewhere.

Great, now you know how to initialize the `HouseData` shared object. Move to the next function call.

In this game, the users must create a `Counter` object to start the game. So there should be a place in the Player column UI to list the existing `Counter` object information for the player to choose. It seems likely that you will reuse the fetching logic for the `Counter` object in several places in your UI, so it’s good practice to isolate this logic into a React hook, which you call `useFetchCounterNft()` in `useFetchCounterNft.ts`:

```typescript title='containers/Player/useFetchCounterNft.ts'
import { useCurrentAccount, useSuiClientQuery } from "@mysten/dapp-kit";
import {} from "react";
import { PACKAGE_ID } from "../../constants";

// React hook to fetch CounterNFT owned by connected wallet
// This hook is to demonstrate how to use `@mysten/dapp-kit` React hook to query data
// besides using SuiClient directly
export function useFetchCounterNft() {
  const account = useCurrentAccount();

  if (!account) {
    return { data: [] };
  }

  // Fetch CounterNFT owned by current connected wallet
  // Only fetch the 1st one
  const { data, isLoading, isError, error, refetch } = useSuiClientQuery(
    "getOwnedObjects",
    {
      owner: account.address,
      limit: 1,
      filter: {
        MatchAll: [
          {
            StructType: `${PACKAGE_ID}::counter_nft::Counter`,
          },
          {
            AddressOwner: account.address,
          },
        ],
      },
      options: {
        showOwner: true,
        showType: true,
      },
    },
    { queryKey: ["CounterNFT"] },
  );

  return {
    data: data && data.data.length > 0 ? data?.data : [],
    isLoading,
    isError,
    error,
    refetch,
  };
}
```

This hook logic is very basic: if there is no current connected wallet, return empty data; otherwise, fetch the `Counter` object and return it. dApp Kit provides a React hook, `useSuiClientQuery()`, that enables interaction with [Sui RPC](references/sui-api.mdx) methods. Different RPC methods require different parameters. To fetch the object owned by a known address, use the [`getOwnedObjects` query](/sui-api-ref#suix_getownedobjects).

Now, pass the address of the connected wallet, as well as the global identifier for the `Counter`. This is in similar format to the global identifier type for function calls:

`${PACKAGE_ID}::counter_nft::Counter`

That’s it, now put the hook into the UI component `PlayerListCounterNft.tsx` and display the data:

```typescript title='containers/Player/PlayerListCounterNft.tsx'
export function PlayerListCounterNft() {
  const { data, isLoading, error, refetch } = useFetchCounterNft();
  const { mutate: execCreateCounterNFT } = useSignAndExecuteTransactionBlock();

  return (
    <Container mb={"4"}>
      <Heading size="3" mb="2">
        Counter NFTs
      </Heading>

      {error && <Text>Error: {error.message}</Text>}

      <Box mb="3">
        {data.length > 0 ? (
          data.map((it) => {
            return (
              <Box key={it.data?.objectId}>
                <Text as="div" weight="bold">
                  Object ID:
                </Text>
                <Text as="div">{it.data?.objectId}</Text>
                <Text as="div" weight="bold">
                  Object Type:
                </Text>
                <Text as="div">{it.data?.type}</Text>
              </Box>
            );
          })
        ) : (
          <Text>No CounterNFT Owned</Text>
        )}
      </Box>

    </Container>
  );
}
```

For the case when there is no existing `Counter` object, mint a new `Counter` for the connected wallet. Also add the minting logic into `PlayerListCounterNft.tsx` when the user clicks the button. You already know how to build and execute a Move call with `TransactionBlock` and `initialize_house_data()`, you can implement a similar call here.

As you might recall with `TransactionBlock`, outputs from the transaction can be inputs for the next transaction. Call `counter_nft::mint()`, which returns the newly created `Counter` object, and use it as input for `counter_nft::transfer_to_sender()` to transfer the `Counter` object to the caller wallet:

```typescript title='containers/Player/PlayerListCounterNft.tsx'
const txb = new TransactionBlock();
const [counterNft] = txb.moveCall({
  target: `${PACKAGE_ID}::counter_nft::mint`,
});
txb.moveCall({
  target: `${PACKAGE_ID}::counter_nft::transfer_to_sender`,
  arguments: [counterNft],
});


execCreateCounterNFT(
  {
    transactionBlock: txb,
  },
  {
    onError: (err) => {
      toast.error(err.message);
    },
    onSuccess: (result) => {
      toast.success(`Digest: ${result.digest}`);
      refetch?.();
    },
  },
);
```

Great, now you can create the game with the created `Counter` object. Isolate the game creation logic into `PlayerCreateGame.tsx`. There is one more thing to keep in mind - to flag an input as an on-chain object, you should use `txb.object()` with the corresponding object ID.

```typescript title='containers/Player/PlayerCreateGame.tsx'
// Create new transaction block
const txb = new TransactionBlock();

// Player stake
const [stakeCoin] = txb.splitCoins(txb.gas, [
  MIST_PER_SUI * BigInt(stake),
]);

// Create the game with CounterNFT
txb.moveCall({
  target: `${PACKAGE_ID}::single_player_satoshi::start_game`,
  arguments: [
    txb.pure.string(guess),
    txb.object(counterNFTData[0].data?.objectId!),
    stakeCoin,
    txb.object(houseDataId),
  ],
});

execCreateGame(
  {
    transactionBlock: txb,
  },
  {
    onError: (err) => {
      toast.error(err.message);
    },
    onSuccess: (result: SuiTransactionBlockResponse) => {
      toast.success(`Digest: ${result.digest}`);
    },
  },
);
```

One final step remains: settle the game. There are a couple of ways you can use the UI to settle the game:

1. Create a Settle Game button and pass all the necessary arguments to the `single_player_satoshi::finish_game()` Move call.
1. Settle the game automatically through an events subscription. This example uses this path to teache good practices on events and how to subscribe to them.

All of this logic is in `HouseFinishGame.tsx`:

```typescript title='containers/House/HouseFinishGame.tsx'
// This component will help the House to automatically finish the game whenever new game is started
export function HouseFinishGame() {
  const suiClient = useSuiClient();
  const { mutate: execFinishGame } = useSignAndExecuteTransactionBlock();

  const [housePrivHex] = useContext(HouseKeypairContext);
  const [houseDataId] = useContext(HouseDataContext);

  useEffect(() => {
    // Subscribe to NewGame event
    const unsub = suiClient.subscribeEvent({
      filter: {
        MoveEventType: `${PACKAGE_ID}::single_player_satoshi::NewGame`,
      },
      onMessage(event) {
        console.log(event);
        const { game_id, vrf_input } = event.parsedJson as {
          game_id: string;
          vrf_input: number[];
        };

        toast.info(`NewGame started ID: ${game_id}`);

        console.log(housePrivHex);

        try {
          const houseSignedInput = bls.sign(
            new Uint8Array(vrf_input),
            curveUtils.hexToBytes(housePrivHex),
          );

          // Finish the game immediately after new game started
          const txb = new TransactionBlock();
          txb.moveCall({
            target: `${PACKAGE_ID}::single_player_satoshi::finish_game`,
            arguments: [
              txb.pure.id(game_id),
              txb.pure(bcs.vector(bcs.U8).serialize(houseSignedInput)),
              txb.object(houseDataId),
            ],
          });
          execFinishGame(
            {
              transactionBlock: txb,
            },
            {
              onError: (err) => {
                toast.error(err.message);
              },
              onSuccess: (result: SuiTransactionBlockResponse) => {
                toast.success(`Digest: ${result.digest}`);
              },
            },
          );
        } catch (err) {
          console.error(err);
        }
      },
    });

    return () => {
      (async () => (await unsub)())();
    };
  }, [housePrivHex, houseDataId, suiClient]);

  return null;
}
```

To get the underlying `SuiClient` instance from the SDK, use `useSuiClient()`. You want to subscribe to events whenever the `HouseFinishGame` component loads. To do this, use the React hook `useEffect()` from the core React library.

`SuiClient` exposes a method called `subscribeEvent()` that enables you to subscribe to a variety of event types. `SuiClient::subscribeEvent()` is actually a thin wrapper around the RPC method [`suix_subscribeEvent`](/sui-api-ref#suix_subscribeevent).

The logic is that whenever a new game starts, you want to settle the game immediately. The necessary event to achieve this is the Move event type called `single_player_satoshi::NewGame`. If you inspect the parsed payload of the event through `event.parsedJson`, you can see the corresponding event fields declared in the smart contract. In this case, you just need to use two fields, the Game ID and the VRF input.

The next steps are similar to the previous Move calls, but you have to use the BLS private key to sign the VRF input and then pass the Game ID, signed VRF input and `HouseData` ID to the `single_player_satoshi::finish_game()` Move call.

Last but not least, remember to unsubscribe from the event whenever the `HouseFinishGame` component dismounts. This is important as you might not want to subscribe to the same event multiple times.

Congratulations, you completed the frontend. You can carry the lessons learned here forward when using the dApp Kit to build your next Sui project.
