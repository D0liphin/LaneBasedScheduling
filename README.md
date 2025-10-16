# Lane Based Scheduling 

A better name is pending.

## Summary

Deep memory hierarchies introduce significant performance challenges due to high miss latency. This is increasingly relevant with CXL, which enables large remote memory pools that expand capacity but are slower than local DRAM. This project addresses the issue by providing tools to relax program ordering, hiding stalls through prefetching and software pipelining to increase program throughput, potentially at the cost of latency.

## Criteria for Success

For a diverse range of practical workloads (see Evaluation), the project is successful if it:

1. Maintains similar throughput as memory latency increases — e.g., achieving comparable performance for local DRAM and remote NUMA memory.

2. Defines latency SLOs and demonstrates the trade-off in latency for individual transactions consumed by the software pipeline.

3. Performs competitively against alternative approaches such as data restructuring or static analysis.

4. Performs competitively against systems emphasizing parallelism, supporting the argument that optimizing for memory behavior is more impactful than fine-grained parallelism.

## Evaluation

* Use a NUMA node to represent CXL memory.

Workload ideas are evolving as we go, but some:

* Use a TPC-C workload to evaluate latency/throughput tradeoff.
* General purpose workloads:
    * Random forest construction and inference
* Can we handle sparse matrix computations more effectively than a polyhedral optimizer?

## Design

'Lanes' allow us to define dependencies between closures. A lane is not an 
independent stream of computation, but a tag that you can give a closure to 
define "happens after" relationships.

Thus you can say two things:

1. This closure can be defined to execute on lane `x`.
2. This closure must happen after all closures previously declared on any of the following lanes: `[a, b, c]`.

```c
void sched(closure_t closure, lanemask_t after, uint8_t on_lane);
```

If it still doesn't make sense, just think of it like this. If you had an infinite number of lanes, you could just give every function its own lane and then define the exact dependencies.

```cpp
uint8_t a_lane = new_lane(),
        b_lane = new_lane(), 
        c_lane = new_lane();
        
sched(a, 0, a_lane);
sched(b, a_lane, b_lane);
sched(c, a_lane, c_lane);
sched(d, b_lane | c_lane, new_lane());
```

The only difference is that you have a finite number of lanes. 
