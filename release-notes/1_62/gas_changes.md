## Sui v1.62 Gas Schedule Updates (and what to expect)

Sui v1.62 includes targeted gas schedule and metering fixes to more accurately account for the true execution cost and introduce a few new gas changes to reduce over-charging.

### Summary of gas schedule changes

**Dynamic field changes (largest behavioral impact)**

Dynamic field operations now charge differently depending on cache status:

- **First load of a dynamic field** (i.e., not in the runtime cache, and not created in the transaction) is **more expensive**.
- **Subsequent loads of the same dynamic field within the transaction** are **significantly cheaper**.
- **Accessing dynamic fields created earlier in the same transaction** are  **significantly cheaper.**

**Additional execution-level adjustments**

The following execution-level adjustments are being introduced:

- **`MoveLoc` is cheaper:** We previously (incorrectly) charged proportionally to value size, even though no value is created. We now charge a constant amount.
- **`ReadRef` is slightly more expensive:** This creates value copies, which we now account for.
- **Execution stack tracking:** More accurate stack-height metering reduces over-charging for some instructions.
- **Primitive size accounting tuning:** The computed “size” for several primitive types now better match their actual size, including some decreases and increases.

---

## Observed impact from transaction sampling

We backtested several million transactions when examining these gas changes. Across a few million sampled transactions:

- **5.7%** saw a change in gas usage. Of that:
    - **1.3%** saw a gas **increase**
    - **4.4%** saw a gas **decrease**
- The **mean** change in gas costs across all transactions was **−6.03%**
- The **median** change in gas costs was **−21.52%**

Looking at the distribution among affected transactions:

- Up through about the **75th percentile**, changes are **net decreases** in gas costs.
- From the **76th percentile onward**, changes shift to **increases**.
- Extremes (0th/100th percentile) show larger swings, mostly explained by dynamic-field behavior interacting with size/caching.

---

## What this means for developers

For most workloads, you will notice no change (only 5.7% are affected from several million transactions). If you do, it is more likely to be a cost decrease.

### Costly patterns

Your transactions may be more expensive if they do **some or all** of the following:

- Read **many unique dynamic fields**
- The fields read are **large in size**
- Each field is read only a few times per transaction

This is because **first-time loads of uncached dynamic fields** are now **more expensive**.

### Cheaper transactions

Your transactions should get cheaper if they do **any** of the following:

- Read **a small set of dynamic fields repeatedly** within the same transaction
- **Create dynamic fields** and then access them again in-transaction

These now benefit from the cache-aware and “created in-transaction” discounts.

### Secondary effects

Most other instruction and stack-metering changes are modest, paving a path toward optimization in compilation. Over time, compiler work to prefer moving locals over copies wherever possible should allow the compiler to be able to take advantage of the reduced cost for `MoveLoc` that is introduced in these changes.
