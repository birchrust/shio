import { Ed25519Keypair } from '@mysten/sui/keypairs/ed25519';
import { Transaction } from '@mysten/sui/transactions';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';

const rpcUrl = getFullnodeUrl('mainnet');
const suiClient = new SuiClient({ url: rpcUrl });
import WebSocket from 'ws';

let key = "";
const SHIO_PACKAGE = "0x1889977f0fb56ae730e7bda8e8e32859ce78874458c74910d36121a81a615123"

const keypair = Ed25519Keypair.fromSecretKey(key);

const connectToShioFeed = () => {
  const url = 'wss://rpc.getshio.com/feed';
  const ws = new WebSocket(url);

  ws.on('open', () => {
    console.log('Connected to Shio Feed WebSocket.');
  });

  ws.on('message', async (data) => {
    try {
      const message = JSON.parse(data.toString());

      if (message.auctionStarted) {

        const tx = new Transaction();
        tx.setSender(keypair.toSuiAddress())
        await buildAndSubmitBid(message.auctionStarted, tx)
      }
    } catch (error) {
      console.error('Error parsing message:', error);
    }
  });

  ws.on('error', (error) => {
    console.error('WebSocket error:', error);
  });

  ws.on('close', (code, reason) => {
    console.log(`WebSocket connection closed. Code: ${code}, Reason: ${reason.toString()}`);
  });

  const sendMessage = (message: object) => {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
    } else {
      console.error('WebSocket is not open. Cannot send message.');
    }
  };

  return {
    ws,
    sendMessage,
  };
};


const shioConnection = connectToShioFeed();

// Function to adjust gas budget to get a larger digest
const adjustGasBudgetForDigest = async (tx: Transaction, targetDigest: string, initialGasBudget: number): Promise<{ tx: Transaction, digest: string, gasBudget: number }> => {
  let currentGasBudget = initialGasBudget;
  let attempts = 0;
  const maxAttempts = 10000;

  while (attempts < maxAttempts) {
    try {
      tx.setGasBudget(currentGasBudget);

      const currentDigest = await tx.getDigest({
        client: suiClient,
      });

      // Lexicographical comparison of digests
      const buf1 = Buffer.from(currentDigest, 'base64');
      const buf2 = Buffer.from(targetDigest, 'base64');

      if (buf1.compare(buf2) > 0) {
        return {
          tx,
          digest: currentDigest,
          gasBudget: currentGasBudget,
        };
      }

      currentGasBudget += 1;
      attempts++;

      if (attempts % 100 === 0) {
        await new Promise((resolve) => setTimeout(resolve, 1));
      }
    } catch (error) {
      console.error("Error adjusting gas budget:", error);
      throw error;
    }
  }

  throw new Error(`Failed to find valid digest after ${maxAttempts} attempts`);
};

// Function to build and submit bid
const buildAndSubmitBid = async (opportunity: any, txb: Transaction) => {
  try {
    const timeRemaining = opportunity.deadlineTimestampMs - Date.now();

    if (timeRemaining <= 0) {
      console.log("Timeout");
      return null
    }

    const coins = await suiClient.getCoins({
      owner: keypair.toSuiAddress(),
      coinType: "0x2::sui::SUI",
    });

    if (coins.data.length === 0) {
      console.error("No SUI coins available");
      return null;
    }
    console.log(opportunity.txDigest)
    // Adjust gas budget to get a larger digest
    const adjustedTx = await adjustGasBudgetForDigest(txb, opportunity.txDigest, opportunity.gasPrice);

    txb.setGasPayment([{
      objectId: coins.data[0].coinObjectId,
      digest: coins.data[0].digest,
      version: coins.data[0].version,
    }]);

    txb.setSender(keypair.getPublicKey().toSuiAddress());

    // 2. Add submit_bid call
    const shioMoveCall = {
      target: `${SHIO_PACKAGE}::auctioneer::submit_bid`,
      arguments: [
        txb.object("0xc32ce42eac951759666cbc993646b72387ec2708a2917c2c6fb7d21f00108c18"),
        txb.pure.u64(adjustedTx.gasBudget), // Use adjusted gas budget
        txb.moveCall({
          target: "0x2::coin::into_balance",
          typeArguments: ["0x2::sui::SUI"],
          arguments: [txb.splitCoins(txb.gas, [adjustedTx.gasBudget.toString()])],
        }),
      ],
      typeArguments: [],
    };

    txb.moveCall(shioMoveCall);

    // 3. Build and sign
    const builtTx = await txb.build({ client: suiClient });
    const signature = await keypair.signTransaction(builtTx);

    // 4. Prepare and send
    const txData = Buffer.from(builtTx).toString('base64');
    const sig = signature.signature;

    const data = {
      oppTxDigest: opportunity.txDigest,
      bidAmount: adjustedTx.gasBudget,
      txData,
      sig
    }
    // shioConnection.sendMessage(data);

    await submitBidByAPI({
      txDigest: opportunity.txDigest,
      bidAmount: adjustedTx.gasBudget,
      txData,
      sig
    })
    return

  } catch (error) {
    console.error("Error in bid submission:", error);
    return null;
  }
};

interface SubmitBidByAPIProps {
  txDigest: string
  bidAmount: number
  txData: string
  sig: string
}

async function submitBidByAPI({ txDigest,
  bidAmount,
  txData,
  sig, }: SubmitBidByAPIProps) {
  const url = "https://rpc.getshio.com";

  const requestData = {
    jsonrpc: "2.0",
    id: 1,
    method: "shio_submitBid",
    params: [txDigest, bidAmount, txData, sig],
  };

  try {
    const response = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(requestData),
    });

    if (!response.ok) {
      const errorResponse = await response.json();
      console.error("Error Response:", errorResponse);
      return;
    }

    const result = await response.json();

    if (result.error) {

      console.error("Your Response:", result.error.message);
      return
    }

    console.log("Success:", result);


  } catch (error) {
    console.error("Error:", error);
  }
}
