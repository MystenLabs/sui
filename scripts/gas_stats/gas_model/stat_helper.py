################################################################
# Helpers for statistics and transactions manipulation.
#

import numpy as np
import pandas as pd

def print_percentiles(memo, txns_data):
    """
    Print statistics for a list of some transaction data (time, gas charges, objects used, etc).
    Uses `memo` as a header when printing.
    Print min, max and typical percentile information.
    """
    print(f"**** {memo} count: {len(txns_data)} ****")
    if len(txns_data) == 0:
        return
    if len(txns_data) < 2:
        print(txns_data)
        return
    percentiles = get_percentiles(txns_data)
    if percentiles is None:
        return
    print(f"min: {percentiles[0]:,}")
    print(f"max: {percentiles[1]:,}")
    print(f" 5.00th percentile: {percentiles[2]:,}")
    print(f"25.00th percentile: {percentiles[3]:,}")
    print(f"50.00th percentile: {percentiles[4]:,}")
    print(f"75.00th percentile: {percentiles[5]:,}")
    print(f"95.00th percentile: {percentiles[6]:,}")
    print(f"99.00th percentile: {percentiles[7]:,}")
    print(f"99.99th percentile: {percentiles[8]:,}")


def get_percentiles(txns_data):
    """
    Given a list of tuples of any kind, loads the first element in the tuple and
    computes statistics on it.
    Return a tuple with (min, max, 5th, 25th, 50th, 75th, 95th, 99th, 99.99th) percentiles.
    When the list has less than 10 elements, return None.
    """
    if len(txns_data) < 10:
        return None
    return (
        min(txns_data),
        max(txns_data),
        np.percentile(txns_data, 5),
        np.percentile(txns_data, 25),
        np.percentile(txns_data, 50),
        np.percentile(txns_data, 75),
        np.percentile(txns_data, 95),
        np.percentile(txns_data, 99),
        np.percentile(txns_data, 99.99)
    )


def correlation(data1, data2):
    """
    Given 2 lists of the same size return the correlation between them.
    For example, to determine the correlation between time and instructions, provide 2 lists,
    one containing "time spent" and the other "number of instructions" for each transaction.
    The returned value is between 1 and -1. With 1 indicating strong correlation, 0 no correlation and
    -1 strong negative correlation.
    """
    correlation_matrix = np.corrcoef(data1, data2)
    return correlation_matrix[0, 1]


def group_by_percentile(data, percentiles, prct_idx, other_idx):
    """
    Given a list of data, expressed as tuples, and a list of percentiles, group the data
    according to the percentile of the data at index `prct_idx`.
    Return a list of pairs with the first element being the value at index `other_idx` and the second
    element being the value at index `prct_idx`.
    As an example consider a list of tuples with the following structure:
    (time, computation cost, total cost, instructions, memory)
    If we want to group the data by percentile of "total cost" and correlate it with "time",
    we would call this function as follows:
    group_by_percentile(data, [10, 25, 50, 75, 90], 2, 0)
    and what is returned is a list of 6 lists, each list containing pairs of (time, total cost)
    for al elements in the given percentile.
    So [
        [list_of_less_then_10_percentile],
        [list_of_between_10_and_25_percentile],
        [list_of_between_25_and_50_percentile],
        [list_of_between_50_and_75_percentile],
        [list_of_between_75_and_90_percentile],
        [list_of_bigger_than_90_percentile],
    ]
    that would allow to analyze group of data according to their percentile, where a given correlation may
    be stronger in one group than in another (particularly for the extremes).
    """
    break_up = []
    break_up_values = []

    # compute percentile over the desired index
    view = [datum[prct_idx] for datum in data]
    for percentile in percentiles:
        break_up_values.append(np.percentile(view, percentile))
        break_up.append([])
    break_up.append([])

    # assign the data in the proper bucket according to percentile
    for datum in data:
        done = False
        for (idx, break_up_value) in enumerate(break_up_values):
            if datum[prct_idx] < break_up_value:
                done = True
                break_up[idx].append((datum[other_idx], datum[prct_idx]))
                break
        if not done:
            break_up[-1].append((datum[other_idx], datum[prct_idx]))

    return break_up


def find_outliers_zscore(data, threshold=2.5):
    """
    Given a list of data, return a list of outliers.
    An outlier is a value that is `threshold` standard deviations away from the mean.
    """
    if len(data) < 2:
        return []
    mean = np.mean(data)
    std = np.std(data)
    if std == 0:
        return []
    z_scores = [(x - mean) / std for x in data]
    outliers = [data[i] for i, z in enumerate(z_scores) if abs(z) > threshold]
    return outliers


def find_outliers_iqr(data, threshold=1.5):
    """
    Given a list of data, return a list of outliers.
    An outlier is a value that is `threshold` interquartile ranges away from the mean.
    """
    if len(data) < 2:
        return []
    quartiles = np.percentile(data, [5, 75])
    iqr = quartiles[1] - quartiles[0]
    lower_bound = quartiles[0] - threshold * iqr
    upper_bound = quartiles[1] + threshold * iqr
    outliers = [x for x in data if x < lower_bound or x > upper_bound]
    return outliers


def trim_outliers(txns, threshold=2):
    """
    Walk through the list of transactions and remove time outliers.
    The list of transactions is modified in place.
    """
    for txn in txns:
        outliers = find_outliers_zscore(txn.times, threshold)
        if outliers:
            txn.times = [t for t in txn.times if t not in outliers]