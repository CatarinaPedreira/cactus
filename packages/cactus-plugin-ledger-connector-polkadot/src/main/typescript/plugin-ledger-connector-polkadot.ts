import { Server } from "http";
import { Server as SecureServer } from "https";
import { ApiPromise, WsProvider } from "@polkadot/api";

import "multer";
import { Optional } from "typescript-optional";

import { PluginRegistry } from "@hyperledger/cactus-core";

import {
  IPluginLedgerConnector,
  ConsensusAlgorithmFamily,
  PluginAspect,
  IPluginWebService,
  IWebServiceEndpoint,
  ICactusPlugin,
  ICactusPluginOptions,
} from "@hyperledger/cactus-core-api";

import {
  Logger,
  Checks,
  LogLevelDesc,
  LoggerProvider,
} from "@hyperledger/cactus-common";
import { promisify } from "util";

// Should work further on this
export interface IPluginLedgerConnectorPolkadotOptions
  extends ICactusPluginOptions {
  logLevel?: LogLevelDesc;
  pluginRegistry: PluginRegistry;
  wsProviderUrl: WsProvider;
  instanceId: string;
}

interface RunTransactionRequest {
  keychainId: string;
  keychainRef: string;
  channelName: string;
  chainCodeId: string;
  invocationType: string; // Change later
  functionName: string;
  functionArgs: Array<string>;
}

interface RunTransactionResponse {
  functionOutput: string;
}

interface DeployContractInkBytecodeRequest {
  inkSource: string; // Change later
  inkMod: string; // Change later
  moduleName: string;
  pinnedDeps: Array<string>;
  modTidyOnly: boolean;
}

interface DeployContractInkBytecodeResponse {
  result: string;
}

export class PluginLedgerConnectorPolkadot
  implements
    IPluginLedgerConnector<
      DeployContractInkBytecodeRequest,
      DeployContractInkBytecodeResponse,
      RunTransactionRequest,
      RunTransactionResponse
    >,
    ICactusPlugin,
    IPluginWebService {
  public static readonly CLASS_NAME = "PluginLedgerConnectorPolkadot";
  public readonly DEFAULT_WSPROVIDER = "wss://rpc.polkadot.io";
  private readonly instanceId: string;
  private readonly log: Logger;
  private wsProvider: WsProvider;

  public get className(): string {
    return PluginLedgerConnectorPolkadot.CLASS_NAME;
  }

  constructor(public readonly opts: IPluginLedgerConnectorPolkadotOptions) {
    const fnTag = `${this.className}#constructor()`;
    Checks.truthy(opts, `${fnTag} arg options`);
    if (typeof opts.logLevel !== "undefined") {
      Checks.truthy(opts.logLevel, `${fnTag} options.logLevelDesc`);
    }
    Checks.truthy(opts.pluginRegistry, `${fnTag} options.pluginRegistry`);
    Checks.truthy(opts.wsProviderUrl, `${fnTag} options.wsProviderUrl`);
    Checks.truthy(opts.instanceId, `${fnTag} options.instanceId`);

    const level = this.opts.logLevel || "INFO";
    const label = this.className;
    this.log = LoggerProvider.getOrCreate({ level, label });

    this.instanceId = opts.instanceId;
    this.wsProvider = new WsProvider(this.DEFAULT_WSPROVIDER);
  }

  public setProvider(wsProviderUrl: string): void {
    this.wsProvider = new WsProvider(wsProviderUrl);
  }

  public async createAPI(): Promise<ApiPromise> {
    const api = await ApiPromise.create({ provider: this.wsProvider });
    this.log.info("create polkadot api");
    return api;
  }

  public async installWebServices(): Promise<IWebServiceEndpoint[]> {
    throw Error("Not implemented yet");
  }

  public async shutdown(): Promise<void> {
    const serverMaybe = this.getHttpServer();
    if (serverMaybe.isPresent()) {
      const server = serverMaybe.get();
      await promisify(server.close.bind(server))();
    }
  }

  public getInstanceId(): string {
    return this.instanceId;
  }

  public getPackageName(): string {
    return `@hyperledger/cactus-plugin-ledger-connector-polkadot`;
  }

  public getAspect(): PluginAspect {
    return PluginAspect.LEDGER_CONNECTOR;
  }

  public getHttpServer(): Optional<Server | SecureServer> {
    return Optional.empty();
  }

  public async getConsensusAlgorithmFamily(): Promise<
    ConsensusAlgorithmFamily
  > {
    return ConsensusAlgorithmFamily.STAKE;
  }
  //
  // public async transact(req: RunTransactionRequest) {
  //   //const fnTag = `${this.className}#transact()`;
  // }
  //
  // // criar a open api para conter os requests
  // public async deployContract(
  // ) {
  //   // const fnTag = `${this.className}#deployContract()`;
  //   // Checks.truthy(req, `${fnTag} req`);
  //   //
  //   // const web3SigningCredential = req.web3SigningCredential; // verificar o que é esta variável exatamente, e qual será a sua forma correta aqui
  //   //
  //   // return this.transact({
  //   //   transactionConfig: {
  //   //     data: `0x${req.bytecode}`,
  //   //     from: web3SigningCredential.ethAccount, // verificar se aqui tb é mesmo ethAccount
  //   //     gas: req.gas,
  //   //     gasPrice: req.gasPrice,
  //   //   },
  //   //   web3SigningCredential,
  //   // });
  // }
}
