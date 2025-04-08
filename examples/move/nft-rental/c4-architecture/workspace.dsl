workspace {
    model {
        renter = person "Renter"
        borrower = person "Borrower"
        blockchain_infra = softwareSystem "Blockchain Infrastructure" {
            renter -> this "Uses"
            borrower -> this "Uses"
            rentables_ext = container "Rentables Extension" {
                renter -> this "Uses"
                borrower -> this "Uses"
                item = component "Item" "This is the bare item we want to rent. It is wrapped inside Rentable."
                rentable = component "Rentable" "An object that wraps an item to be rented. Holds additional information of the rental policies."
                promise = component "Promise" "A Promise object given to the borrower when they want to use the borrowed item. Acts as a hot potato, requiring that the rentable be returned after use."
                install = component "Install" "Allows for a borrower or renter to install the Rentables Extension to their Kiosk."
                list = component "List" "Allows for a renter to list an item for renting."
                delist = component "Delist" "Allows a renter to delist an item they are currently listing but not actively renting."
                rent = component "Rent" "Allows a borrower to rent an item that is being listed in one's Rentables Extension."
                borrow = component "Borrow" "Enables the borrower to borrow the item either by value of by reference. Depending on the configuration defined by the renter."
                reclaim = component "Reclaim" "Allows the renter to claim their item after the rental period has expired."
                
                # component relationships
                item -> rentable "Is wrapped inside the"
                list -> rentable "Creates a"
                delist -> item "Releases the"
                borrow -> item "Grants access temporarily to"
                borrow -> promise "Produces a"
                reclaim -> item "Releases the"
                rentable -> rent "Is transferred to borrower's Kiosk Extension"

                # person relationships
                renter -> install "Invokes"
                renter -> list "Invokes"
                renter -> delist "Invokes"
                renter -> reclaim "Invokes"
                borrower -> install "Invokes"
                borrower -> rent "Invokes"
                # rent -> borrower "Receives in Kiosk Extension"
                borrower  -> borrow "Invokes"
            }
                            
            kiosk_ext = container "Kiosk Extension" {
                rentables_ext -> this "Uses"
            }
            kiosk = container "Kiosk" {
                kiosk_ext -> this "Uses"
            }
            blockchain = container "Blockchain"
        }

    }
    views {
        systemContext blockchain_infra {
            include *
            autolayout lr
        }
        container blockchain_infra {
            include *
            autolayout lr
        }
        component rentables_ext {
            include *
            autolayout lr
        }
        theme default
    }
}
